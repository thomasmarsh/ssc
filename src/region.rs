//! Regions: the names of places. A region is what the player thinks of as "where I am":
//! the dominant species range of a sector, a civilization's territory, a barren gap between
//! ranges, a belt of rock, a lonely planetoid's oasis, or a rare convergence of many species.
//! Names are spelled from phoneme tables (soft syllables for wildlife, harsh ones for
//! civilizations, which `Territory::name` shares), so they are a pure function of the seed
//! and a sector, with the same name everywhere the same range dominates.

use crate::range::{ecology, ring};
use crate::world::{SectorId, hash2};
use std::fmt::Write as _;

/// A range counts toward a convergence zone when it is at least this present.
pub const CONFLUENCE_WEIGHT: f32 = 0.35;
/// Species it takes to make a convergence zone (rare: see `range` tests).
pub const CONFLUENCE_SPECIES: usize = 4;
/// Below this life a sector is a sparse gap; a planetoid there makes an oasis.
pub const GAP_LIFE: f32 = 0.2;
pub const OASIS_LIFE: f32 = 0.35;
/// A rock belt is rich in matter and short of life.
pub const BELT_MATTER: f32 = 0.65;
pub const BELT_LIFE: f32 = 0.45;
/// Gaps, belts and oases are named per block of this many sectors, so a stretch of them
/// shares a name (an oasis and the gap around it share one, with their own word).
pub const BLOCK: i32 = 3;

const NAME_SALT: u64 = 0x4E41_4D45_0000_003B;

/// Soft syllables for wildlife: open, flowing.
pub const SOFT_ONSET: [&str; 12] = [
    "l", "m", "n", "v", "s", "f", "w", "y", "h", "el", "al", "or",
];
pub const SOFT_VOWEL: [&str; 8] = ["a", "e", "i", "o", "u", "ae", "ia", "ou"];
pub const SOFT_CODA: [&str; 8] = ["", "", "n", "l", "r", "s", "th", "m"];
/// Harsh syllables for civilizations: stops and clusters.
pub const HARSH_ONSET: [&str; 12] = [
    "k", "g", "z", "th", "kr", "dr", "vr", "x", "gr", "b", "tz", "zh",
];
pub const HARSH_VOWEL: [&str; 6] = ["a", "o", "u", "i", "ak", "or"];
pub const HARSH_CODA: [&str; 8] = ["k", "rk", "x", "th", "g", "z", "rd", "gor"];

fn pick<'a>(table: &[&'a str], bits: &mut u64) -> &'a str {
    let item = table[(*bits % table.len() as u64) as usize];
    *bits /= table.len() as u64;
    item
}

fn capitalized(mut text: String) -> String {
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

fn spell(hash: u64, onset: &[&str], vowel: &[&str], coda: &[&str], harsh: bool) -> String {
    let mut bits = hash;
    // Two syllables, and a third about a time in three.
    let syllables = 2 + usize::from(bits.is_multiple_of(3));
    bits /= 3;
    let mut out = String::new();
    for index in 0..syllables {
        out.push_str(pick(onset, &mut bits));
        out.push_str(pick(vowel, &mut bits));
        // Harsh names close every syllable; soft ones only sometimes, and never the first.
        if harsh || index > 0 {
            out.push_str(pick(coda, &mut bits));
        }
    }
    capitalized(out)
}

/// A soft, flowing name for a place of wildlife.
pub fn soft_name(hash: u64) -> String {
    spell(hash, &SOFT_ONSET, &SOFT_VOWEL, &SOFT_CODA, false)
}

/// A harsh name for a civilization.
pub fn harsh_name(hash: u64) -> String {
    spell(hash, &HARSH_ONSET, &HARSH_VOWEL, &HARSH_CODA, true)
}

/// What kind of place a region is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    /// HOME, the start.
    Home,
    /// The range of a dominant species.
    Wild,
    /// Where many species' ranges converge.
    Confluence,
    /// A rock-rich band with little life.
    Belt,
    /// A sparse gap between ranges.
    Gap,
    /// A planetoid with life around it, in the middle of nowhere.
    Oasis,
    /// A civilization's territory.
    Civ,
}

/// A named place. The key is the same wherever the same region extends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub key: u64,
    pub name: String,
    pub kind: RegionKind,
}

/// What a dominant range is called (picked from its identity, so it never changes with the
/// drift of the genome across the range).
const WILD_WORDS: [&str; 8] = [
    "Drifts", "Steppe", "Marches", "Wilds", "Shoals", "Meadows", "Expanse", "Pastures",
];

/// The name of a civilization with its title, from its identity (shared with
/// `Territory::name`): harsh syllables, then the shape's word.
pub fn civ_name(id: u64, title: &str) -> String {
    format!("{} {title}", harsh_name(hash2(id ^ NAME_SALT, 7, 3)))
}

fn block(id: SectorId) -> (i32, i32) {
    (id.x.div_euclid(BLOCK), id.y.div_euclid(BLOCK))
}

/// The region sector `id` belongs to. Pure.
pub fn region(seed: u64, id: SectorId) -> Region {
    if ring(id) == 0 {
        return Region {
            key: 1,
            name: "Homestead".into(),
            kind: RegionKind::Home,
        };
    }
    if let Some(t) = crate::territory::territory(seed, id) {
        return Region {
            key: t.id,
            name: capitalized(t.name(seed).to_lowercase()),
            kind: RegionKind::Civ,
        };
    }
    let eco = ecology(seed, id);
    let named = |tag: u64, a: i32, b: i32| hash2(seed ^ NAME_SALT ^ tag, a, b);
    let crowd = eco
        .presence
        .iter()
        .filter(|p| p.weight >= CONFLUENCE_WEIGHT)
        .count();
    let (kind, key, suffix) = if crowd >= CONFLUENCE_SPECIES {
        let lead = &eco.presence[0];
        let key = named(
            1,
            (lead.center.x * 8.0) as i32,
            (lead.center.y * 8.0) as i32,
        );
        (RegionKind::Confluence, key, "Confluence".to_string())
    } else if eco.presence.is_empty() || eco.life < GAP_LIFE {
        if crate::world::has_planetoid(seed, id) && eco.life < OASIS_LIFE {
            let (bx, by) = block(id);
            (RegionKind::Oasis, named(2, bx, by), "Oasis".to_string())
        } else {
            let (bx, by) = block(id);
            (RegionKind::Gap, named(3, bx, by), "Reach".to_string())
        }
    } else if eco.matter >= BELT_MATTER && eco.life < BELT_LIFE {
        let (bx, by) = block(id);
        (RegionKind::Belt, named(4, bx, by), "Belt".to_string())
    } else if crate::world::has_planetoid(seed, id) && eco.life < OASIS_LIFE {
        let (bx, by) = block(id);
        (RegionKind::Oasis, named(2, bx, by), "Oasis".to_string())
    } else {
        let lead = &eco.presence[0];
        let key = named(
            5 ^ lead.species.lineage,
            (lead.center.x * 8.0) as i32,
            (lead.center.y * 8.0) as i32,
        );
        let word = WILD_WORDS[((key >> 17) % WILD_WORDS.len() as u64) as usize];
        (RegionKind::Wild, key, word.to_string())
    };
    let mut name = soft_name(key);
    let _ = write!(name, " {suffix}");
    Region { key, name, kind }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: u64 = 0x535343;

    fn map(reach: i32) -> Vec<(SectorId, Region)> {
        let mut out = Vec::new();
        for x in -reach..=reach {
            for y in -reach..=reach {
                let id = SectorId { x, y };
                out.push((id, region(SEED, id)));
            }
        }
        out
    }

    #[test]
    fn naming_is_deterministic_and_seeded() {
        assert_eq!(map(10), map(10));
        let id = SectorId { x: 9, y: -7 };
        assert_ne!(region(1, id).name, region(2, id).name);
        for seed in 0..4 {
            assert_eq!(region(seed, id), region(seed, id));
        }
        assert_eq!(soft_name(12345), soft_name(12345));
        assert_ne!(soft_name(12345), harsh_name(12345));
    }

    #[test]
    fn names_are_wellformed_and_reasonably_unique() {
        let regions = map(30);
        let mut by_key: std::collections::HashMap<u64, &str> = Default::default();
        let mut by_name: std::collections::HashMap<&str, std::collections::HashSet<u64>> =
            Default::default();
        for (_, r) in &regions {
            assert!(!r.name.is_empty() && r.name.is_ascii());
            assert!(!r.name.contains('—') && r.name.chars().next().unwrap().is_uppercase());
            // One key, one name.
            assert_eq!(*by_key.entry(r.key).or_insert(&r.name), r.name);
            by_name.entry(&r.name).or_default().insert(r.key);
        }
        let regions_seen = by_key.len();
        let clashes = by_name.values().filter(|keys| keys.len() > 1).count();
        assert!(regions_seen > 150, "{regions_seen} regions");
        assert!(
            (clashes as f32) < regions_seen as f32 * 0.03,
            "{clashes} of {regions_seen} names are shared by different regions"
        );
    }

    #[test]
    fn wildlife_names_are_soft_and_civilization_names_harsh() {
        let harsh = ['k', 'z', 'x', 'g'];
        let (mut wild, mut civ) = (0, 0);
        for (_, r) in map(40) {
            let first = r.name.split(' ').next().unwrap().to_lowercase();
            match r.kind {
                RegionKind::Civ => {
                    civ += 1;
                    assert!(
                        first
                            .chars()
                            .any(|c| harsh.contains(&c) || c == 'b' || c == 'd')
                    );
                }
                RegionKind::Home => {}
                _ => {
                    wild += 1;
                    assert!(
                        !first.chars().any(|c| harsh.contains(&c)),
                        "{first} is not soft"
                    );
                }
            }
        }
        assert!(wild > 500 && civ > 20, "{wild} {civ}");
    }

    #[test]
    fn special_forms_and_the_dominant_range_name_places() {
        let all = map(40);
        let kinds = |kind: RegionKind| {
            all.iter()
                .filter(|(_, r)| r.kind == kind)
                .collect::<Vec<_>>()
        };
        for (kind, suffix) in [
            (RegionKind::Gap, " Reach"),
            (RegionKind::Belt, " Belt"),
            (RegionKind::Oasis, " Oasis"),
            (RegionKind::Confluence, " Confluence"),
        ] {
            let found = kinds(kind);
            assert!(!found.is_empty(), "no {kind:?} in the map");
            assert!(found.iter().all(|(_, r)| r.name.ends_with(suffix)));
        }
        // Convergence is rare, the dominant ranges are the bulk.
        let (wild, confluence) = (
            kinds(RegionKind::Wild).len(),
            kinds(RegionKind::Confluence).len(),
        );
        assert!(confluence * 10 < wild, "{confluence} vs {wild}");
        // Neighbouring sectors of one range share its name: far fewer names than sectors.
        let names: std::collections::HashSet<_> =
            kinds(RegionKind::Wild).iter().map(|(_, r)| r.key).collect();
        assert!(
            names.len() * 2 < wild,
            "{} regions over {wild} sectors",
            names.len()
        );
        // HOME is the Homestead.
        assert_eq!(region(SEED, SectorId::ORIGIN).name, "Homestead");
    }
}
