//! Canonical state digest: a deterministic, order-stable fingerprint of everything that decides
//! what the simulation does next, split into named sub-digests so a changed fingerprint says
//! WHICH subsystem diverged. It exists to prove "no behavior change" across refactors (see
//! `docs/PERF.md` and the Guarantees section of `docs/MIGRATION.md`).
//!
//! Design:
//! - Hand-listed at the `Game` level, generic below it. `Game::digest_parts` destructures `Game`
//!   with no `..`, so adding a field to `Game` is a compile error until it is classified into a
//!   sub-digest (or listed as excluded with a reason). Types below that level are hashed through
//!   their derived `Debug` output, so a new field on a body, genome or pad is covered
//!   automatically.
//! - Floats are hashed by their exact `Debug` text (shortest round-trip, so distinct bit patterns
//!   give distinct text; `-0.0` differs from `0.0`; NaN payloads are not distinguished).
//! - `HashMap`/`HashSet` iteration order never leaks: maps owned by `Game` are sorted by key
//!   before hashing, and types that hold a hash map hand-sort it in their own `digest` helper
//!   (see `digest_pad` and friends below).
//! - Field names are stripped from the `Debug` text (`Body { id: 7, ..}` hashes as `Body { 7, ..}`)
//!   so renaming a field does not change a digest. Reordering fields, moving them between
//!   structs or changing a type's variants does, and needs a deliberate re-bless.
//! - FNV-1a, 64 bit, implemented here; no dependency. On demand only: nothing runs in `step`.

use super::*;
use std::fmt::{Debug, Write as _};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The named sub-digests of a game. Compare field by field to see which subsystem diverged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateDigest {
    /// Simulation time, the master seed, every random stream, id counters and global clocks.
    pub clock: u64,
    /// Live bodies in order, and everything attached to bodies (chains, tethers, flocks, adaptation,
    /// powers, parasites, builds, parry/dash/jam state, contact gaps).
    pub bodies: u64,
    /// Bullets, mines, pickups, rune fields, rifts, song rings, eggs and plankton.
    pub projectiles: u64,
    /// The ship's persistent record: loadout, cargo, pads, jobs, chart, run record, legacy, score,
    /// lives, dev toggles, tuning overrides (only when not the default), feel and ping state.
    pub ship: u64,
    /// Civilizations met: territories, lineages, bases, brains, regard, societies, fauna, raids,
    /// apex elders.
    pub civilization: u64,
    /// The world deltas and what is loaded: fallen spawns, mined ore, relics, loaded and active
    /// sectors, farm.
    pub world: u64,
    /// Cues, visual effects, notices and announcement flags: outputs for the adapter. Reported
    /// separately so a presentation-only change is recognisable as such.
    pub presentation: u64,
}

impl StateDigest {
    /// Every sub-digest with its name, in a fixed order.
    pub fn parts(&self) -> [(&'static str, u64); 7] {
        [
            ("clock", self.clock),
            ("bodies", self.bodies),
            ("projectiles", self.projectiles),
            ("ship", self.ship),
            ("civilization", self.civilization),
            ("world", self.world),
            ("presentation", self.presentation),
        ]
    }

    /// One number covering every sub-digest.
    pub fn combined(&self) -> u64 {
        let mut hasher = Hasher::new("combined");
        for (_, part) in self.parts() {
            hasher.u64(part);
        }
        hasher.finish()
    }

    /// Names of the sub-digests that differ from `other`.
    pub fn differing(&self, other: &StateDigest) -> Vec<&'static str> {
        self.parts()
            .into_iter()
            .zip(other.parts())
            .filter(|((_, a), (_, b))| a != b)
            .map(|((name, _), _)| name)
            .collect()
    }
}

/// FNV-1a over a stream of values, with a reusable text buffer.
struct Hasher {
    state: u64,
    buf: String,
}

impl Hasher {
    fn new(label: &str) -> Self {
        let mut hasher = Self {
            state: FNV_OFFSET,
            buf: String::new(),
        };
        hasher.bytes(label.as_bytes());
        hasher
    }

    fn bytes(&mut self, bytes: &[u8]) {
        let mut state = self.state;
        for &byte in bytes {
            state = (state ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
        }
        self.state = state;
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    /// Hashes the `Debug` text of a value with field names stripped, then a separator.
    fn put<T: Debug + ?Sized>(&mut self, value: &T) {
        self.buf.clear();
        let _ = write!(self.buf, "{value:?}");
        let text = std::mem::take(&mut self.buf);
        self.absorb(text.as_bytes());
        self.buf = text;
        self.bytes(&[0x1f]);
    }

    /// Hashes Debug text, skipping `identifier: ` field names outside string literals.
    fn absorb(&mut self, text: &[u8]) {
        let len = text.len();
        let mut i = 0;
        let mut start = 0;
        let mut in_string = false;
        while i < len {
            let c = text[i];
            if in_string {
                if c == b'\\' {
                    i += 1;
                } else if c == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }
            if c == b'"' {
                in_string = true;
                i += 1;
                continue;
            }
            let ident_start = c.is_ascii_alphabetic() || c == b'_';
            let word_start =
                i == 0 || !(text[i - 1].is_ascii_alphanumeric() || text[i - 1] == b'_');
            if ident_start && word_start {
                let mut end = i;
                while end < len && (text[end].is_ascii_alphanumeric() || text[end] == b'_') {
                    end += 1;
                }
                if end + 1 < len && text[end] == b':' && text[end + 1] == b' ' {
                    self.bytes(&text[start..i]);
                    i = end + 2;
                    start = i;
                } else {
                    i = end;
                }
                continue;
            }
            i += 1;
        }
        self.bytes(&text[start..len]);
    }

    /// A hash map in sorted key order, so iteration order never matters.
    fn map<K: Ord + Debug, V: Debug>(&mut self, map: &HashMap<K, V>) {
        let mut pairs: Vec<(&K, &V)> = map.iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        self.u64(pairs.len() as u64);
        for (key, value) in pairs {
            self.put(key);
            self.put(value);
        }
    }

    /// A hash set in sorted order.
    fn set<K: Ord + Debug>(&mut self, set: &HashSet<K>) {
        let mut keys: Vec<&K> = set.iter().collect();
        keys.sort();
        self.u64(keys.len() as u64);
        for key in keys {
            self.put(key);
        }
    }

    fn finish(self) -> u64 {
        self.state
    }
}

impl Game {
    /// The canonical digest of this game's state. On demand only; never called by `step`.
    pub fn state_digest(&self) -> StateDigest {
        let Game {
            bodies,
            bullets,
            effects,
            cues,
            tethers,
            chains,
            pools,
            pickups,
            mines,
            rune_fields,
            engulf,
            engulf_free,
            rifts,
            rift_traces,
            loadout,
            home_input_orders,
            jobs,
            stats,
            notices,
            bench_feedback,
            unlock_guidance,
            unlock_announced,
            unlock_pending,
            cargo,
            pad,
            parry,
            dash,
            impact_gap,
            drone_impact_gap,
            gripped,
            relics_taken,
            farm,
            veil,
            parasites,
            builds,
            parry_rng,
            ping,
            chart,
            legacy,
            bequest,
            beam,
            electrolysis,
            score,
            run,
            // A cache of pure functions of the seed, not state.
            lore: _,
            lives,
            dev,
            tune,
            tune_regen,
            game_over,
            // Wall-clock profiling, not state.
            #[cfg(feature = "profile")]
                profile: _,
            time,
            player_invulnerability,
            focus,
            food,
            eggs,
            territory,
            territory_name,
            raid,
            territory_sector,
            civs,
            apexes: apex_group,
            fauna,
            jam,
            splits,
            song_rings,
            seed,
            rng,
            loot,
            variation,
            growth,
            breeding,
            food_clock,
            arm_clock,
            switch_clock,
            arsenal_flash,
            next_id,
            next_chain,
            fallen,
            mined,
            mined_contents,
            regrow_stamp,
            mine_clock,
            mine_target,
            mine_note,
            loaded,
            active,
            flocks,
            sanctuary,
            region,
            realms,
            streak,
            feel,
            lure,
        } = self;

        let civstate::Civs {
            lineages,
            bases,
            works,
            mining,
            colors,
            territories,
            fall,
            brains,
            regard,
            societies,
            hits,
            struck,
            clock: civ_clock,
            rng: civ_rng,
        } = civs;

        let apexes::Apexes {
            info: apexes,
            seen: apex_seen,
            state: apex_state,
            power: power_state,
            rng: apex_rng,
            adapt,
        } = apex_group;

        let mut h = Hasher::new("clock");
        h.put(seed);
        h.put(time);
        h.put(player_invulnerability);
        h.put(game_over);
        h.put(civ_clock);
        h.put(food_clock);
        h.put(arm_clock);
        h.put(switch_clock);
        h.put(mine_clock);
        h.put(mine_note);
        h.put(next_id);
        h.put(next_chain);
        for stream in [
            rng, loot, variation, growth, breeding, parry_rng, civ_rng, apex_rng,
        ] {
            h.put(stream);
        }
        let clock = h.finish();

        let mut h = Hasher::new("bodies");
        h.put(bodies);
        h.put(chains);
        h.put(pools);
        h.put(tethers);
        h.put(flocks);
        h.put(splits);
        h.put(adapt);
        h.map(apex_state);
        h.map(power_state);
        h.put(engulf);
        h.put(engulf_free);
        h.put(gripped);
        h.put(veil);
        h.put(jam);
        h.put(parry);
        h.put(dash);
        h.map(impact_gap);
        h.map(drone_impact_gap);
        h.put(mine_target);
        h.put(beam);
        h.put(electrolysis);
        h.put(parasites);
        h.put(builds);
        let bodies = h.finish();

        let mut h = Hasher::new("projectiles");
        h.put(bullets);
        h.put(mines);
        h.put(pickups);
        h.put(rune_fields);
        h.put(rifts);
        h.put(song_rings);
        h.put(eggs);
        h.put(food);
        let projectiles = h.finish();

        let mut h = Hasher::new("ship");
        h.put(loadout);
        h.put(stats);
        h.put(cargo);
        h.put(jobs);
        h.put(home_input_orders);
        h.put(pad);
        h.put(chart);
        h.put(run);
        h.put(legacy);
        h.put(bequest);
        h.put(score);
        h.put(lives);
        h.put(dev);
        h.put(streak);
        h.put(feel);
        h.put(lure);
        // The sonar cache is a pure function of the seed and is left out.
        h.put(&ping.cooldown);
        h.put(&ping.ring);
        h.put(&ping.echoes);
        // Tuning joins only when it is not the default, so an untouched run digests as it
        // always did and two runs with different tuning can never compare equal.
        if *tune != tuning::Tunables::DEFAULT {
            h.put(&tune.overrides());
        }
        if *tune_regen {
            h.put("tune_regen");
        }
        let ship = h.finish();

        let mut h = Hasher::new("civilization");
        h.map(lineages);
        h.map(bases);
        h.map(works);
        h.put(mining);
        h.map(colors);
        h.map(territories);
        h.map(fall);
        h.map(brains);
        h.put(regard);
        h.put(societies);
        h.put(hits);
        h.map(struck);
        h.put(fauna);
        h.put(territory);
        h.put(territory_name);
        h.put(raid);
        h.put(territory_sector);
        h.put(apexes);
        h.set(apex_seen);
        let civilization = h.finish();

        let mut h = Hasher::new("world");
        let mut sectors: Vec<_> = fallen.iter().collect();
        sectors.sort_by_key(|(sector, _)| **sector);
        h.u64(sectors.len() as u64);
        for (sector, spawns) in sectors {
            h.put(sector);
            h.set(spawns);
        }
        h.map(mined);
        h.map(mined_contents);
        h.map(regrow_stamp);
        h.set(relics_taken);
        h.set(loaded);
        h.put(active);
        h.put(focus);
        h.put(farm);
        h.put(sanctuary);
        h.put(region);
        h.put(realms);
        let world = h.finish();

        let mut h = Hasher::new("presentation");
        h.put(cues);
        h.put(effects);
        h.put(rift_traces);
        h.put(notices);
        h.put(bench_feedback);
        h.put(unlock_guidance);
        h.put(unlock_announced);
        h.put(unlock_pending);
        h.put(arsenal_flash);
        let presentation = h.finish();

        StateDigest {
            clock,
            bodies,
            projectiles,
            ship,
            civilization,
            world,
            presentation,
        }
    }
}

/// A `HashMap` whose `Debug` output is in sorted key order. Use it for any map held by a type that
/// the digest hashes through `Debug`, so hash iteration order cannot leak into a digest. Behaves
/// exactly like the wrapped map otherwise.
#[derive(Clone)]
pub struct DetMap<K, V>(HashMap<K, V>);

impl<K: Eq + Hash, V: PartialEq> PartialEq for DetMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K, V> Default for DetMap<K, V> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<K, V> Deref for DetMap<K, V> {
    type Target = HashMap<K, V>;
    fn deref(&self) -> &HashMap<K, V> {
        &self.0
    }
}

impl<K, V> DerefMut for DetMap<K, V> {
    fn deref_mut(&mut self) -> &mut HashMap<K, V> {
        &mut self.0
    }
}

impl<K: Eq + Hash, V> FromIterator<(K, V)> for DetMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<K: Ord + Debug, V: Debug> Debug for DetMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut entries: Vec<(&K, &V)> = self.0.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        f.debug_map().entries(entries).finish()
    }
}

/// A `HashSet` whose `Debug` output is in sorted order; see `DetMap`.
#[derive(Clone)]
pub struct DetSet<K>(HashSet<K>);

impl<K: Eq + Hash> PartialEq for DetSet<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K> Default for DetSet<K> {
    fn default() -> Self {
        Self(HashSet::new())
    }
}

impl<K> Deref for DetSet<K> {
    type Target = HashSet<K>;
    fn deref(&self) -> &HashSet<K> {
        &self.0
    }
}

impl<K> DerefMut for DetSet<K> {
    fn deref_mut(&mut self) -> &mut HashSet<K> {
        &mut self.0
    }
}

impl<K: Ord + Debug> Debug for DetSet<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut keys: Vec<&K> = self.0.iter().collect();
        keys.sort();
        f.debug_set().entries(keys).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Changing one representative field moves exactly the sub-digest that owns it.
    #[test]
    fn a_changed_field_moves_only_its_own_sub_digest() {
        let base = Game::new(7);
        let digest = base.state_digest();
        assert_eq!(
            digest,
            Game::new(7).state_digest(),
            "same game, same digest"
        );

        let mut game = Game::new(7);
        game.bodies[0].health -= 1.0;
        assert_eq!(digest.differing(&game.state_digest()), ["bodies"]);

        let mut game = Game::new(7);
        game.score += 1;
        assert_eq!(digest.differing(&game.state_digest()), ["ship"]);

        let mut game = Game::new(7);
        game.mined.insert((SectorId::ORIGIN, 3), 1.0);
        assert_eq!(digest.differing(&game.state_digest()), ["world"]);

        let mut game = Game::new(7);
        game.rng.next_u64();
        assert_eq!(digest.differing(&game.state_digest()), ["clock"]);
    }

    /// Field names do not matter, values and string contents do, and a map hashes the same
    /// whatever order it was filled in.
    #[test]
    fn hashing_ignores_names_and_map_order_but_not_values() {
        mod a {
            #[derive(Debug)]
            #[allow(dead_code)]
            pub struct P {
                pub x: f32,
            }
        }
        mod b {
            #[derive(Debug)]
            #[allow(dead_code)]
            pub struct P {
                pub renamed: f32,
            }
        }
        let hash = |value: &dyn Debug| {
            let mut h = Hasher::new("t");
            h.put(value);
            h.finish()
        };
        assert_eq!(hash(&a::P { x: 1.5 }), hash(&b::P { renamed: 1.5 }));
        assert_ne!(hash(&a::P { x: 1.5 }), hash(&b::P { renamed: 1.25 }));
        assert_ne!(hash(&0.0f32), hash(&-0.0f32));
        assert_ne!(hash(&"a: 1"), hash(&"b: 1"));
        let forward: DetMap<u64, f32> = (0..50).map(|k| (k, k as f32)).collect();
        let backward: DetMap<u64, f32> = (0..50).rev().map(|k| (k, k as f32)).collect();
        assert_eq!(hash(&forward), hash(&backward));
    }
}
