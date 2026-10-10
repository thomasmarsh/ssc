//! Headless performance baseline: runs the pinned scenarios (`ssc::simulation::scenario`) and
//! prints per-tick timings, population and the end digest. See `docs/PERF.md`.
//!
//! usage: simperf [--only NAME] [--json] [--goldens]
//!   --only NAME  run one scenario (repeatable); default: all
//!   --json       machine-readable output, one JSON object with a `scenarios` array
//!   --goldens    print the pinned-digest table for `src/simulation/goldens.rs` and exit

use ssc::simulation::scenario::{self, Run, SCENARIOS};
use std::process::ExitCode;
use std::time::Instant;

struct Report {
    name: &'static str,
    ticks: u32,
    total_ms: f64,
    mean_ms: f64,
    p50_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    bodies: usize,
    bullets: usize,
    loaded: usize,
    digest: String,
    golden_ok: bool,
    /// Mean ms per tick of each phase; only with `--features profile`.
    #[cfg(feature = "profile")]
    phases: Vec<(&'static str, f64)>,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let at = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[at.min(sorted.len() - 1)]
}

fn measure(scenario: &'static scenario::Scenario) -> (Report, Vec<scenario::Checkpoint>) {
    let mut run = Run::new(scenario);
    let mut per_tick = Vec::with_capacity(scenario.ticks as usize);
    let mut checkpoints = Vec::new();
    let mut next = scenario.checkpoints.iter().copied().peekable();
    loop {
        let tick_start = Instant::now();
        let more = run.tick();
        let spent = tick_start.elapsed();
        if !more {
            break;
        }
        per_tick.push(spent.as_secs_f64() * 1000.0);
        // Digest time is excluded from the tick timings (and from `total_ms`).
        while next.peek() == Some(&run.ticks_done()) {
            next.next();
            checkpoints.push(scenario::Checkpoint {
                tick: run.ticks_done(),
                digest: run.game().state_digest(),
            });
        }
    }
    let total_ms = per_tick.iter().sum::<f64>();
    let mut sorted = per_tick.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let game = run.game();
    let digest = checkpoints.last().map_or(0, |c| c.digest.combined());
    let report = Report {
        name: scenario.name,
        ticks: scenario.ticks,
        total_ms,
        mean_ms: total_ms / f64::from(scenario.ticks.max(1)),
        p50_ms: percentile(&sorted, 0.50),
        p99_ms: percentile(&sorted, 0.99),
        max_ms: sorted.last().copied().unwrap_or(0.0),
        bodies: game.bodies.len(),
        bullets: game.bullets.len(),
        loaded: run.loaded_sectors(),
        digest: format!("{digest:#018x}"),
        golden_ok: scenario::verify(scenario.name, &checkpoints).is_ok(),
        #[cfg(feature = "profile")]
        phases: game.phase_timings(),
    };
    (report, checkpoints)
}

fn main() -> ExitCode {
    let mut only: Vec<String> = Vec::new();
    let mut json = false;
    let mut goldens = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--only" => match args.next() {
                Some(name) => only.push(name),
                None => return usage("--only needs a name"),
            },
            "--json" => json = true,
            "--goldens" => goldens = true,
            "-h" | "--help" => return usage(""),
            other => return usage(&format!("unknown argument: {other}")),
        }
    }
    for name in &only {
        if scenario::find(name).is_none() {
            return usage(&format!("no scenario named {name}"));
        }
    }
    let chosen = SCENARIOS
        .iter()
        .filter(|s| only.is_empty() || only.iter().any(|n| n == s.name));

    if goldens {
        println!("pub const GOLDENS: &[Golden] = &[");
        for s in chosen {
            let checkpoints = scenario::run(s);
            print!("{}", scenario::golden_lines(s.name, &checkpoints));
        }
        println!("];");
        return ExitCode::SUCCESS;
    }

    let reports: Vec<Report> = chosen.map(|s| measure(s).0).collect();
    if json {
        println!("{{\"scenarios\": [");
        for (i, r) in reports.iter().enumerate() {
            println!(
                "  {{\"name\": \"{}\", \"ticks\": {}, \"total_ms\": {:.3}, \"mean_ms\": {:.4}, \
                 \"p50_ms\": {:.4}, \"p99_ms\": {:.4}, \"max_ms\": {:.4}, \"bodies\": {}, \
                 \"bullets\": {}, \"loaded_sectors\": {}, \"digest\": \"{}\", \"golden_ok\": {}}}{}",
                r.name,
                r.ticks,
                r.total_ms,
                r.mean_ms,
                r.p50_ms,
                r.p99_ms,
                r.max_ms,
                r.bodies,
                r.bullets,
                r.loaded,
                r.digest,
                r.golden_ok,
                if i + 1 < reports.len() { "," } else { "" }
            );
        }
        println!("]}}");
    } else {
        println!(
            "{:<12} {:>6} {:>10} {:>9} {:>9} {:>9} {:>9} {:>6} {:>7} {:>6}  {:<20} golden",
            "scenario",
            "ticks",
            "total ms",
            "mean ms",
            "p50 ms",
            "p99 ms",
            "max ms",
            "bodies",
            "bullets",
            "loaded",
            "digest"
        );
        for r in &reports {
            println!(
                "{:<12} {:>6} {:>10.1} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>6} {:>7} {:>6}  {:<20} {}",
                r.name,
                r.ticks,
                r.total_ms,
                r.mean_ms,
                r.p50_ms,
                r.p99_ms,
                r.max_ms,
                r.bodies,
                r.bullets,
                r.loaded,
                r.digest,
                if r.golden_ok { "ok" } else { "MISMATCH" }
            );
        }
    }
    #[cfg(feature = "profile")]
    if !json {
        print_phases(&reports);
    }
    if reports.iter().all(|r| r.golden_ok) {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "simperf: digests differ from the pinned goldens; behavior changed (or a different platform)"
        );
        ExitCode::FAILURE
    }
}

/// Mean ms per tick by phase, one column per scenario. Includes the timing calls themselves
/// (two clock reads per phase), so the sum slightly exceeds the untimed tick.
#[cfg(feature = "profile")]
fn print_phases(reports: &[Report]) {
    println!("\nmean ms per tick by phase (profile build)");
    print!("{:<14}", "phase");
    for r in reports {
        print!(" {:>12}", r.name);
    }
    println!();
    let rows = reports.first().map_or(0, |r| r.phases.len());
    for i in 0..rows {
        print!("{:<14}", reports[0].phases[i].0);
        for r in reports {
            print!(" {:>12.5}", r.phases[i].1);
        }
        println!();
    }
    print!("{:<14}", "total");
    for r in reports {
        print!(" {:>12.5}", r.phases.iter().map(|p| p.1).sum::<f64>());
    }
    println!();
}

fn usage(why: &str) -> ExitCode {
    if !why.is_empty() {
        eprintln!("{why}");
    }
    eprintln!("usage: simperf [--only NAME]... [--json] [--goldens]");
    if why.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
