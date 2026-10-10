//! Headless difficulty measurement: spawned organisms and sectors across rings and realms,
//! printed as markdown tables. See `ssc::threat` and `docs/BALANCE.md`.
//!
//! usage: threat [--seed N] [--seeds K] [--rings 0,1,2,..] [--per-ring N] [--top N]
//!               [--realm-rings A..B] [--only SECTION]
//!   --seed N        master seed, decimal or 0x hex (default: the game's master seed)
//!   --seeds K       measure K seeds (the master seed and K-1 derived ones), default 1
//!   --rings LIST    rings to tabulate (default 0,1,2,3,4,5,6,8,10,14,20,30)
//!   --per-ring N    sectors sampled per ring per seed (default 24)
//!   --top N         rows in the outlier tables (default 15)
//!   --realm-rings   ring span sampled for the per-realm table (default 40..130, about 25 rings)
//!   --only SECTION  one of rings, outliers, weapons, speed, tiers, realms (repeatable)

use ssc::config::MASTER_SEED;
use ssc::threat::{
    Class, Dist, Organism, SectorReport, Tier, assess_sector, ring_sectors, tier_at, tier_bare,
};
use ssc::world::{self, SectorId};
use std::collections::BTreeMap;
use std::process::ExitCode;

struct Options {
    seed: u64,
    seeds: u32,
    rings: Vec<i32>,
    per_ring: usize,
    top: usize,
    realm_rings: (i32, i32),
    only: Vec<String>,
}

fn parse_seed(text: &str) -> Result<u64, String> {
    let parsed = match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.map_err(|_| format!("bad seed: {text}"))
}

fn parse() -> Result<Options, String> {
    let mut o = Options {
        seed: MASTER_SEED,
        seeds: 1,
        rings: vec![0, 1, 2, 3, 4, 5, 6, 8, 10, 14, 20, 30],
        per_ring: 24,
        top: 15,
        realm_rings: (40, 130),
        only: Vec::new(),
    };
    let mut args = std::env::args().skip(1);
    let need = |v: Option<String>, n: &str| v.ok_or_else(|| format!("{n} needs a value"));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => o.seed = parse_seed(&need(args.next(), "--seed")?)?,
            "--seeds" => {
                o.seeds = need(args.next(), "--seeds")?
                    .parse()
                    .map_err(|_| "bad --seeds".to_string())?
            }
            "--rings" => {
                o.rings = need(args.next(), "--rings")?
                    .split(',')
                    .map(|r| r.trim().parse().map_err(|_| format!("bad ring {r}")))
                    .collect::<Result<_, _>>()?
            }
            "--per-ring" => {
                o.per_ring = need(args.next(), "--per-ring")?
                    .parse()
                    .map_err(|_| "bad --per-ring".to_string())?
            }
            "--top" => {
                o.top = need(args.next(), "--top")?
                    .parse()
                    .map_err(|_| "bad --top".to_string())?
            }
            "--realm-rings" => {
                let text = need(args.next(), "--realm-rings")?;
                let (a, b) = text.split_once("..").ok_or("--realm-rings wants A..B")?;
                o.realm_rings = (
                    a.parse().map_err(|_| "bad --realm-rings".to_string())?,
                    b.parse().map_err(|_| "bad --realm-rings".to_string())?,
                );
            }
            "--only" => o.only.push(need(args.next(), "--only")?),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(o)
}

fn derived(seed: u64, k: u32) -> u64 {
    if k == 0 {
        seed
    } else {
        world::hash2(seed, k as i32, 0x7B_i32)
    }
}

fn wants(o: &Options, section: &str) -> bool {
    o.only.is_empty() || o.only.iter().any(|s| s == section)
}

use ssc::threat::is_hostile as hostile;

fn f(v: f32) -> String {
    if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

fn trio(d: Dist) -> String {
    format!("{} / {} / {}", f(d.p50), f(d.p99), f(d.max))
}

fn print_rings(o: &Options, by_ring: &BTreeMap<i32, Vec<(u64, SectorReport)>>) {
    println!("## Per ring\n");
    println!(
        "Per organism (hostile and armed): burst potential is every shot of every armed part in {:.0} s, enraged; expected is the share that lands at half range on a {}-unit ship. Cells are p50 / p99 / max.\n",
        ssc::threat::WINDOW,
        ssc::threat::SHIP_RADIUS
    );
    println!(
        "| ring | sectors | organisms | danger | power | burst potential | burst expected | max hit | dps | pool | bare-ship ttk (s) | speed/460 |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (ring, list) in by_ring {
        let all: Vec<&Organism> = list.iter().flat_map(|(_, s)| &s.organisms).collect();
        let hostile: Vec<&Organism> = all.iter().copied().filter(|o| hostile(o)).collect();
        let col = |g: &dyn Fn(&Organism) -> f32| Dist::of(hostile.iter().map(|o| g(o)).collect());
        let danger = Dist::of(list.iter().map(|(_, s)| s.danger).collect());
        println!(
            "| {ring} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            list.len(),
            all.len(),
            trio(danger),
            trio(col(&|o| o.power)),
            trio(col(&|o| o.burst_potential)),
            trio(col(&|o| o.burst_expected)),
            trio(col(&|o| o.max_hit)),
            trio(col(&|o| o.dps)),
            trio(col(&|o| o.pool)),
            trio(col(&|o| o.ttk)),
            trio(col(&|o| o.speed / 460.0)),
        );
    }
    println!();
    let _ = o;
}

fn print_lethal(o: &Options, by_ring: &BTreeMap<i32, Vec<(u64, SectorReport)>>) {
    println!("## Sectors that can end a ship inside one window\n");
    println!(
        "Share of sampled sectors holding at least one hostile organism whose window burst is at least the ship's hull plus shield. `exp` counts shots that land at half range, `pot` every shot. Tiers: bare ship; a typical kit (14 rolls) and a maxed kit (400 Epic rolls) as found at that ring's depth; maxed with a supplier's equipment grade of that depth.\n"
    );
    println!(
        "| ring | bare exp | bare pot | typical exp | typical pot | maxed exp | maxed pot | maxed+grade exp | maxed+grade pot | maxed pool |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let share = |list: &[(u64, SectorReport)], tier: &Tier, expected: bool| {
        let hit = list
            .iter()
            .filter(|(_, s)| s.lethal_for(tier, expected))
            .count();
        format!("{:.0}%", 100.0 * hit as f32 / list.len().max(1) as f32)
    };
    for (ring, list) in by_ring {
        let d = *ring as f32;
        let bare = tier_bare();
        let typical = tier_at(o.seed, d, false, false);
        let maxed = tier_at(o.seed, d, true, false);
        let graded = tier_at(o.seed, d, true, true);
        println!(
            "| {ring} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} |",
            share(list, &bare, true),
            share(list, &bare, false),
            share(list, &typical, true),
            share(list, &typical, false),
            share(list, &maxed, true),
            share(list, &maxed, false),
            share(list, &graded, true),
            share(list, &graded, false),
            maxed.pool(0.0),
        );
    }
    println!();
    println!(
        "### Sectors in rings up to 14 that would kill a maxed ship (no supplier grade) inside one window\n"
    );
    println!(
        "| ring | sector | realm | burst exp | burst pot | maxed pool | weapon x volley | armed | radius | pool | name |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    let mut rows = 0;
    for (ring, list) in by_ring.iter().filter(|(r, _)| **r <= 14) {
        let maxed = tier_at(o.seed, *ring as f32, true, false);
        let mut per = 0;
        for (_, s) in list {
            let worst = s
                .organisms
                .iter()
                .filter(|org| hostile(org) && org.burst_ratio(&maxed) >= 1.0)
                .max_by(|a, b| a.burst_expected.total_cmp(&b.burst_expected));
            if let Some(org) = worst
                && per < 3
                && rows < 40
            {
                per += 1;
                rows += 1;
                println!(
                    "| {ring} | {},{} | {} | {} | {} | {:.0} | {:?} x {} | {} | {:.0} | {:.0} | {} |",
                    s.id.x,
                    s.id.y,
                    s.realm,
                    f(org.burst_expected),
                    f(org.burst_potential),
                    maxed.pool(org.pith),
                    org.weapon,
                    org.volley,
                    org.armed_parts,
                    org.radius,
                    org.pool,
                    org.name
                );
            }
        }
    }
    println!();
}

fn print_outliers(o: &Options, by_ring: &BTreeMap<i32, Vec<(u64, SectorReport)>>) {
    let mut all: Vec<(&SectorReport, &Organism)> = by_ring
        .values()
        .flatten()
        .flat_map(|(_, s)| s.organisms.iter().map(move |org| (s, org)))
        .filter(|(_, org)| hostile(org))
        .collect();
    all.sort_by(|a, b| b.1.burst_potential.total_cmp(&a.1.burst_potential));
    // One row per (lineage, weapon, volley, ring); `n` counts the individuals met.
    let mut seen: BTreeMap<(u64, String, u8, u32), usize> = BTreeMap::new();
    for (s, org) in &all {
        *seen
            .entry((org.lineage, format!("{:?}", org.weapon), org.volley, s.ring))
            .or_default() += 1;
    }
    let mut shown = std::collections::HashSet::new();
    all.retain(|(s, org)| {
        shown.insert((org.lineage, format!("{:?}", org.weapon), org.volley, s.ring))
    });
    println!("## Worst window bursts\n");
    println!(
        "| burst pot | burst exp | class | weapon x volley | armed parts | per shot | period (s) enraged | flight (s) | range / sight | ring | realm | name |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (s, org) in all.iter().take(o.top) {
        println!(
            "| {} | {} | {} | {:?} x {} | {} | {} | {:.2} ({:.2}) | {:.2} | {:.0} / {:.0} | {} | {} | {} |",
            f(org.burst_potential),
            f(org.burst_expected),
            org.class.label(),
            org.weapon,
            org.volley,
            org.armed_parts,
            f(org.shot_damage),
            org.period,
            org.period_enraged,
            org.flight_time,
            org.range,
            org.sight,
            s.ring,
            s.realm,
            org.name
        );
    }
    println!();
    let mut hits: Vec<(&SectorReport, &Organism)> = all.clone();
    hits.sort_by(|a, b| b.1.max_hit.total_cmp(&a.1.max_hit));
    println!("## Hardest single hits (one projectile, mine or bite)\n");
    println!("| max hit | class | weapon x volley | contact | sharpness | ring | realm | name |");
    println!("|---|---|---|---|---|---|---|---|");
    for (s, org) in hits.iter().take(o.top) {
        println!(
            "| {} | {} | {:?} x {} | {} | {:.2} | {} | {} | {} |",
            f(org.max_hit),
            org.class.label(),
            org.weapon,
            org.volley,
            f(org.contact_hit),
            org.sharpness,
            s.ring,
            s.realm,
            org.name
        );
    }
    println!();
    let mut tiny: Vec<(&SectorReport, &Organism)> = all
        .iter()
        .copied()
        .filter(|(_, org)| org.radius <= 16.0 && org.pool <= 80.0)
        .collect();
    tiny.sort_by(|a, b| b.1.burst_potential.total_cmp(&a.1.burst_potential));
    println!("## Tiny things with big bursts (radius <= 16, pool <= 80)\n");
    println!("| burst pot | radius | pool | weapon x volley | armed | ring | realm | name |");
    println!("|---|---|---|---|---|---|---|---|");
    for (s, org) in tiny.iter().take(o.top) {
        println!(
            "| {} | {:.0} | {:.0} | {:?} x {} | {} | {} | {} | {} |",
            f(org.burst_potential),
            org.radius,
            org.pool,
            org.weapon,
            org.volley,
            org.armed_parts,
            s.ring,
            s.realm,
            org.name
        );
    }
    println!();
    println!("## Worst expected burst per ring (rings up to 14)\n");
    println!(
        "| ring | first volley | burst exp | burst pot | pool | weapon x volley | armed | radius | sharp | class | realm | sector | name |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for ring in by_ring.keys().filter(|r| **r <= 14) {
        let mut here: Vec<&(&SectorReport, &Organism)> =
            all.iter().filter(|(s, _)| s.ring as i32 == *ring).collect();
        here.sort_by(|a, b| b.1.burst_expected.total_cmp(&a.1.burst_expected));
        for (s, org) in here.into_iter().take(2) {
            println!(
                "| {} | {} | {} | {} | {:.0} | {:?} x {} | {} | {:.0} | {:.2} | {} | {} | {},{} | {} |",
                s.ring,
                f(org.volley_damage * org.armed_parts as f32),
                f(org.burst_expected),
                f(org.burst_potential),
                org.pool,
                org.weapon,
                org.volley,
                org.armed_parts,
                org.radius,
                org.sharpness,
                org.class.label(),
                s.realm,
                s.id.x,
                s.id.y,
                org.name
            );
        }
    }
    println!();
}

fn print_weapons(by_ring: &BTreeMap<i32, Vec<(u64, SectorReport)>>) {
    let bare = tier_bare();
    let mut by: BTreeMap<String, Vec<(u32, &Organism)>> = BTreeMap::new();
    for (_, s) in by_ring.values().flatten() {
        for org in s.organisms.iter().filter(|o| hostile(o)) {
            by.entry(format!("{:?}", org.weapon))
                .or_default()
                .push((s.ring, org));
        }
    }
    println!("## By weapon\n");
    println!(
        "`first ring` is the nearest ring where a single organism of the weapon could end a bare ship in one window (potential).\n"
    );
    println!(
        "| weapon | organisms | first ring | volley p50 / max | shot dmg p50 / max | burst pot p50 / p99 / max | share >= bare pool | range over sight | power p50 / p99 / max |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for (weapon, list) in by {
        let col = |g: &dyn Fn(&Organism) -> f32| Dist::of(list.iter().map(|(_, o)| g(o)).collect());
        let volley = col(&|o| f32::from(o.volley));
        let shot = col(&|o| o.shot_damage);
        let burst = col(&|o| o.burst_potential);
        let over = list
            .iter()
            .filter(|(_, o)| o.potential_ratio(&bare) >= 1.0)
            .count();
        let first = list
            .iter()
            .filter(|(_, o)| o.potential_ratio(&bare) >= 1.0)
            .map(|(r, _)| *r)
            .min();
        println!(
            "| {weapon} | {} | {} | {} / {} | {} / {} | {} | {:.0}% | {:.0}% | {} |",
            list.len(),
            first.map_or("-".to_string(), |r| r.to_string()),
            f(volley.p50),
            f(volley.max),
            f(shot.p50),
            f(shot.max),
            trio(burst),
            100.0 * over as f32 / list.len() as f32,
            100.0 * list.iter().filter(|(_, o)| o.range > o.sight).count() as f32
                / list.len() as f32,
            trio(col(&|o| o.power)),
        );
    }
    println!();
}

fn print_speed(by_ring: &BTreeMap<i32, Vec<(u64, SectorReport)>>) {
    println!("## Speed\n");
    println!("Alert top speed over the ship's 460 (all organisms with a genome, any class).\n");
    let mut classes: BTreeMap<Class, Vec<f32>> = BTreeMap::new();
    let mut ratios = Vec::new();
    let mut cruise = Vec::new();
    let mut accel = Vec::new();
    for (_, s) in by_ring.values().flatten() {
        for org in s.organisms.iter().filter(|o| o.class != Class::Structure) {
            let r = org.speed / 460.0;
            classes.entry(org.class).or_default().push(r);
            ratios.push(r);
            cruise.push(org.cruise / 460.0);
            accel.push(org.accel);
        }
    }
    println!("| class | n | min | p10 | p50 | p90 | p99 | max |");
    println!("|---|---|---|---|---|---|---|---|");
    let q = |v: &Vec<f32>| {
        let mut s = v.clone();
        s.sort_by(f32::total_cmp);
        let at = |p: f32| s[((s.len() - 1) as f32 * p).round() as usize];
        (at(0.0), at(0.1), at(0.5), at(0.9), at(0.99), at(1.0))
    };
    let mut rows: Vec<(String, Vec<f32>)> = classes
        .into_iter()
        .map(|(c, v)| (c.label().to_string(), v))
        .collect();
    rows.push(("ALL alert".into(), ratios.clone()));
    rows.push(("ALL cruise".into(), cruise));
    for (label, v) in rows {
        if v.is_empty() {
            continue;
        }
        let (a, b, c, d, e, g) = q(&v);
        println!(
            "| {label} | {} | {a:.2} | {b:.2} | {c:.2} | {d:.2} | {e:.2} | {g:.2} |",
            v.len()
        );
    }
    println!();
    let total = ratios.len().max(1) as f32;
    println!("| speed/460 | share of organisms |");
    println!("|---|---|");
    let bins = [0.0, 0.15, 0.25, 0.35, 0.5, 0.75, 1.0, 1.5, f32::MAX];
    for w in bins.windows(2) {
        let n = ratios.iter().filter(|r| **r >= w[0] && **r < w[1]).count();
        let hi = if w[1] == f32::MAX {
            "up".to_string()
        } else {
            format!("{:.2}", w[1])
        };
        println!("| {:.2} to {hi} | {:.0}% |", w[0], 100.0 * n as f32 / total);
    }
    let a = Dist::of(accel);
    println!(
        "\nAcceleration (units/s^2, speed x 2.2 x tow): p50 {} / p90 {} / max {}.\n",
        f(a.p50),
        f(a.p90),
        f(a.max)
    );
}

fn print_tiers(o: &Options) {
    println!("## Player tiers\n");
    println!(
        "Kits rolled with `roll_part` at the depth's threat grade (1 + 0.3 per sector). `power` is `Loadout::power` (bare ship 1); `verdict ratio` is `power / threat^0.8` as the HUD computes it (EVEN is 0.85 to 1.3).\n"
    );
    println!(
        "| tier | hull | shield | recharge/s | guard | dps | damage | volley-free power | threat at depth | verdict ratio |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let mut tiers = vec![(tier_bare(), 0.0f32)];
    for d in [1.0f32, 3.0, 6.0, 10.0, 20.0, 40.0] {
        tiers.push((tier_at(o.seed, d, false, false), d));
        tiers.push((tier_at(o.seed, d, true, false), d));
        tiers.push((tier_at(o.seed, d, true, true), d));
    }
    for (t, d) in tiers {
        let threat = world::threat(d);
        println!(
            "| {} | {:.0} | {:.0} | {:.1} | {:.2} | {:.0} | {:.0} | {:.2} | {:.1} | {:.2} |",
            t.label,
            t.hull(),
            t.shield(),
            t.stats.recharge,
            t.stats.guard,
            t.dps(),
            t.stats.damage,
            t.power,
            threat,
            t.power / threat.powf(0.8)
        );
    }
    println!();
}

fn print_realms(o: &Options) {
    println!("## Per realm\n");
    println!(
        "Sectors of rings {}..{} (about 25 evenly spaced rings, up to {} per ring per seed), grouped by realm kind. Cells are p50 / p99 / max over organisms (burst, power) or p50 / p90 / max over sectors (danger).\n",
        o.realm_rings.0, o.realm_rings.1, o.per_ring
    );
    println!(
        "| realm | sectors | danger | power | burst potential | burst expected | pool | speed/460 |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    let mut by: BTreeMap<&'static str, Vec<SectorReport>> = BTreeMap::new();
    for k in 0..o.seeds {
        let seed = derived(o.seed, k);
        let step = ((o.realm_rings.1 - o.realm_rings.0) / 24).max(1) as usize;
        for r in (o.realm_rings.0..=o.realm_rings.1).step_by(step) {
            for id in ring_sectors(r, o.per_ring) {
                let s = assess_sector(seed, id);
                by.entry(s.realm).or_default().push(s);
            }
        }
    }
    for (realm, list) in by {
        let all: Vec<&Organism> = list.iter().flat_map(|s| &s.organisms).collect();
        let h: Vec<&Organism> = all.iter().copied().filter(|o| hostile(o)).collect();
        let col = |g: &dyn Fn(&Organism) -> f32| Dist::of(h.iter().map(|o| g(o)).collect());
        let danger = Dist::of(list.iter().map(|s| s.danger).collect());
        println!(
            "| {realm} | {} | {} / {} / {} | {} | {} | {} | {} | {} |",
            list.len(),
            f(danger.p50),
            f(danger.p90),
            f(danger.max),
            trio(col(&|o| o.power)),
            trio(col(&|o| o.burst_potential)),
            trio(col(&|o| o.burst_expected)),
            trio(col(&|o| o.pool)),
            trio(col(&|o| o.speed / 460.0)),
        );
    }
    println!();
}

fn print_dominant(o: &Options, by_ring: &BTreeMap<i32, Vec<(u64, SectorReport)>>) {
    println!("## Most dangerous sampled sectors\n");
    println!("| danger | ring | sector | realm | share | dominant contributors |");
    println!("|---|---|---|---|---|---|");
    let mut all: Vec<&SectorReport> = by_ring.values().flatten().map(|(_, s)| s).collect();
    all.sort_by(|a, b| b.danger.total_cmp(&a.danger));
    for s in all.iter().take(o.top) {
        let parts: Vec<String> = s
            .contributors(3)
            .iter()
            .map(|(org, share)| {
                format!(
                    "{} {:?}x{} {:.0}%",
                    org.name,
                    org.weapon,
                    org.volley,
                    share * 100.0
                )
            })
            .collect();
        println!(
            "| {} | {} | {},{} | {} | threat {:.1} | {} |",
            f(s.danger),
            s.ring,
            s.id.x,
            s.id.y,
            s.realm,
            s.threat,
            parts.join("; ")
        );
    }
    println!();
}

fn main() -> ExitCode {
    let o = match parse() {
        Ok(o) => o,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("{message}");
            }
            eprintln!(
                "usage: threat [--seed N] [--seeds K] [--rings 0,1,..] [--per-ring N] [--top N] [--realm-rings A..B] [--only rings|outliers|weapons|speed|tiers|realms]"
            );
            return ExitCode::from(2);
        }
    };
    let mut by_ring: BTreeMap<i32, Vec<(u64, SectorReport)>> = BTreeMap::new();
    for k in 0..o.seeds {
        let seed = derived(o.seed, k);
        for &r in &o.rings {
            for id in ring_sectors(r, o.per_ring) {
                by_ring
                    .entry(r)
                    .or_default()
                    .push((seed, assess_sector(seed, id)));
            }
        }
    }
    let sectors: usize = by_ring.values().map(Vec::len).sum();
    println!(
        "# threat: seed {:#x}, {} seed(s), {} sectors\n",
        o.seed, o.seeds, sectors
    );
    if wants(&o, "rings") {
        print_rings(&o, &by_ring);
        print_lethal(&o, &by_ring);
        print_dominant(&o, &by_ring);
    }
    if wants(&o, "outliers") {
        print_outliers(&o, &by_ring);
    }
    if wants(&o, "weapons") {
        print_weapons(&by_ring);
    }
    if wants(&o, "speed") {
        print_speed(&by_ring);
    }
    if wants(&o, "tiers") {
        print_tiers(&o);
    }
    if wants(&o, "realms") {
        print_realms(&o);
    }
    let _ = SectorId::ORIGIN;
    ExitCode::SUCCESS
}
