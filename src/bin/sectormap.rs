//! Writes an offline sector map (one self-contained HTML file). See `ssc::sectormap`.

use ssc::sectormap::{MAX_SIDE, MapOptions, nearest_keepers, render};
use ssc::world::SectorId;
use std::process::ExitCode;

const USAGE: &str = "usage: sectormap [--seed N] [--cols C] [--rows R] [--center X,Y] [--out FILE]\n  \
    seed: decimal or 0x hex (default: the game's master seed); cols, rows: 1 to {MAX} (default 41);\n  \
    center: sector at the middle of the grid (default 0,0); out: default sectormap.html\n  \
    --nearest-keeper [--from X,Y] [--radius R]: print the realm keepers within R sectors (default\n  \
    300) of X,Y (default: HOME 0,0), nearest first, instead of writing a map";

/// What the command line asked for.
struct Args {
    options: MapOptions,
    out: String,
    nearest: bool,
    from: Option<SectorId>,
    radius: u32,
}

fn sector(name: &str, text: &str) -> Result<SectorId, String> {
    let (x, y) = text.split_once(',').ok_or(format!("{name} wants X,Y"))?;
    Ok(SectorId {
        x: x.trim().parse().map_err(|_| format!("bad {name} x"))?,
        y: y.trim().parse().map_err(|_| format!("bad {name} y"))?,
    })
}

fn number(name: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{name} needs a value"))
}

fn seed(text: &str) -> Result<u64, String> {
    let parsed = match text.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.map_err(|_| format!("bad seed: {text}"))
}

fn parse() -> Result<Args, String> {
    let mut options = MapOptions::default();
    let mut out = "sectormap.html".to_string();
    let (mut nearest, mut from, mut radius) = (false, None, 300u32);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => options.seed = seed(&number("--seed", args.next())?)?,
            "--cols" => {
                options.cols = number("--cols", args.next())?
                    .parse()
                    .map_err(|_| "bad --cols".to_string())?
            }
            "--rows" => {
                options.rows = number("--rows", args.next())?
                    .parse()
                    .map_err(|_| "bad --rows".to_string())?
            }
            "--center" => options.center = sector("--center", &number("--center", args.next())?)?,
            "--from" => from = Some(sector("--from", &number("--from", args.next())?)?),
            "--nearest-keeper" => nearest = true,
            "--radius" => {
                radius = number("--radius", args.next())?
                    .parse()
                    .map_err(|_| "bad --radius".to_string())?
            }
            "--out" => out = number("--out", args.next())?,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        options,
        out,
        nearest,
        from,
        radius,
    })
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(parsed) => parsed,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("sectormap: {message}");
            }
            eprintln!("{}", USAGE.replace("{MAX}", &MAX_SIDE.to_string()));
            return ExitCode::from(2);
        }
    };
    if args.nearest {
        let from = args.from.unwrap_or(args.options.center);
        let hits = nearest_keepers(args.options.seed, from, args.radius);
        println!(
            "seed {}: {} keeper(s) within {} sectors of ({},{})",
            args.options.seed,
            hits.len(),
            args.radius,
            from.x,
            from.y
        );
        for hit in &hits {
            println!("{}", hit.line(from));
        }
        return ExitCode::SUCCESS;
    }
    let Args { options, out, .. } = args;
    let html = match render(options) {
        Ok(html) => html,
        Err(message) => {
            eprintln!("sectormap: {message}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = std::fs::write(&out, &html) {
        eprintln!("sectormap: cannot write {out}: {error}");
        return ExitCode::FAILURE;
    }
    println!(
        "wrote {out}: seed {}, {}x{} sectors, {} KiB",
        options.seed,
        options.cols,
        options.rows,
        html.len() / 1024
    );
    ExitCode::SUCCESS
}
