//! Selected-sector details used by the visual chart's sidebar (the map itself is `chartview`).
use super::{CYAN, DRY_RED, MUTED, PAD_GREEN, lifted};
use crate::Session;
use bevy::prelude::*;
use ssc::simulation::price_text;
use ssc::world::SectorId;

#[derive(Component)]
pub(crate) struct ChartSpan(pub(crate) usize);
const CHART_DETAIL: usize = 16;

pub(crate) fn update_chart(
    session: Res<Session>,
    mut spans: Query<(&mut TextSpan, &mut TextColor, &ChartSpan)>,
) {
    let lines = chart_lines(&session);
    for (mut span, mut color, line) in &mut spans {
        match lines.get(line.0) {
            Some((text, tint)) => {
                if span.0 != *text {
                    span.0 = text.clone();
                }
                color.0 = *tint;
            }
            None => {
                if !span.0.is_empty() {
                    span.0.clear();
                }
            }
        }
    }
}

/// Details for the selected sector; the map itself is drawn by `chartview`.
pub(super) fn chart_lines(session: &Session) -> Vec<(String, Color)> {
    let game = &session.game;
    let Some(cursor) = session.chart else {
        return Vec::new();
    };
    let entries = game.chart_entries();
    let find = |id: SectorId| entries.iter().find(|e| e.sector == id);
    let light = Color::srgb(0.82, 0.88, 0.95);
    let mut detail: Vec<(String, Color)> = Vec::new();
    let depth = ssc::world::latent(game.seed(), cursor.sector).depth;
    let entry = find(cursor.sector);
    let state = match entry {
        Some(e) if e.visited => "VISITED",
        Some(_) => "PINGED",
        None => "UNCHARTED",
    };
    detail.push((
        format!(
            "\nSECTOR ({}, {})   {state}   depth {depth:.1}\n",
            cursor.sector.x, cursor.sector.y
        ),
        light,
    ));
    // A charted sector (visited or pinged) shows the name of the region it lies in.
    if entry.is_some() {
        detail.push((
            format!("REGION  {}\n", game.region_of(cursor.sector).name),
            light,
        ));
        let realm = game.realm_of(cursor.sector);
        let [r, g, b] = realm.tint();
        detail.push((
            format!("REALM   {}   {}\n", realm.name, realm.title()),
            Color::srgb(r, g, b),
        ));
    }
    // How the wildlife of a charted sector in or beside a claim regards that civilization.
    if entry.is_some()
        && let Some((name, mood)) = game.sector_mood(cursor.sector)
        && let Some(read) = mood.read()
    {
        detail.push((
            format!(
                "WILDLIFE toward {name}: {}   ({:.0}% hostile, {:.0}% friendly)\n",
                read.to_uppercase(),
                mood.hostile * 100.0,
                mood.friendly * 100.0
            ),
            light,
        ));
    }
    if let Some(e) = entry {
        if let Some(c) = e.civ {
            let what = if c.capital { "CAPITAL" } else { "OUTPOST" };
            let fallen = if c.fallen { "  FALLEN" } else { "" };
            let regard = match c.regard {
                Some(tier) if !c.fallen => format!("  regard {}", tier.label()),
                None if !c.fallen => "  regard UNMET".to_string(),
                _ => String::new(),
            };
            detail.push((
                format!(
                    "Civilization {what}  threat {}{regard}{fallen}\n",
                    c.threat.label()
                ),
                lifted(Some(c.tint)),
            ));
            if let Some(rule) = c.engagement {
                detail.push((format!("Engagement: {}\n", rule.label()), light));
            }
            if let Some(relation) = c.relationship {
                detail.push((format!("{}\n", relation.text()), light));
            }
            if let Some(culture) = c.culture {
                detail.push((
                    format!("Tends toward {} (contact estimate)\n", culture.tendency),
                    light,
                ));
                if let Some(reason) = culture.last_response {
                    detail.push((format!("Last response: {reason}\n"), light));
                }
            }
        }
        if e.relics > 0 || e.dynamic_wells > 0 {
            let mut discoveries = Vec::new();
            if e.relics > 0 {
                discoveries.push(format!("Sealed organs {}", e.relics));
            }
            if e.dynamic_wells > 0 {
                let modes = e
                    .well_modes
                    .iter()
                    .map(|m| m.label())
                    .collect::<Vec<_>>()
                    .join(", ");
                discoveries.push(format!(
                    "Well anchors {} ({modes}); positions change, rescan nearby",
                    e.dynamic_wells
                ));
            }
            detail.push((format!("{}\n", discoveries.join("   ")), CYAN));
        }
        let mut res = Vec::new();
        if e.planetoids > 0 {
            res.push(format!("Planetoids {}", e.planetoids));
        }
        if e.renewable > 0 {
            res.push(format!("Renewable {}", e.renewable));
        }
        if e.lodes > 0 {
            res.push(format!("Rich lodes {}", e.lodes));
        }
        if !res.is_empty() {
            detail.push((
                format!("{}\n", res.join("   ")),
                Color::srgb(0.95, 0.8, 0.5),
            ));
        }
        let mut life = Vec::new();
        if let Some(n) = e.predators {
            life.push(format!("Predators {n}"));
        }
        if e.nests > 0 {
            life.push(format!("Nests {}", e.nests));
        }
        if e.eggs > 0 {
            life.push(format!("Eggs {}", e.eggs));
        }
        if !life.is_empty() {
            detail.push((format!("{}\n", life.join("   ")), DRY_RED));
        }
        let mut works = Vec::new();
        if e.pads > 0 {
            works.push(format!("Pads {}", e.pads));
        }
        if e.beacons > 0 {
            works.push(format!("Beacons {}", e.beacons));
        }
        if e.wreck {
            works.push("Your wreck".to_string());
        }
        if let Some(pin) = e.pin {
            works.push(format!("Pin: {}", pin.label()));
        }
        if !works.is_empty() {
            detail.push((format!("{}\n", works.join("   ")), PAD_GREEN));
        }
    }
    // The jump from the ship to a beacon in this sector.
    if let Some(id) = game.beacon_in(cursor.sector) {
        if let Some(q) = game.travel_quote(id) {
            let cool = game.travel_cooldown();
            let text = if cool > 0.0 {
                format!(
                    "J jump  {:.0} sectors  recharging {:.0}s\n",
                    q.sectors,
                    cool.ceil()
                )
            } else {
                format!(
                    "J jump  {:.0} sectors  {}  charge {:.0}s\n",
                    q.sectors,
                    price_text(&q.price()),
                    q.charge.ceil()
                )
            };
            detail.push((
                text,
                if cool > 0.0 || !game.cargo.can_afford(&q.price()) {
                    DRY_RED
                } else {
                    CYAN
                },
            ));
        }
    } else if game.beacon_limit() > 0 || !game.beacons().is_empty() {
        detail.push((
            format!(
                "BEACONS {}/{}   H deploys one here\n",
                game.beacons().len(),
                game.beacon_limit()
            ),
            MUTED,
        ));
    }
    detail.push((
        format!(
            "\nNOTE  < {} >    [ ] pick   F pin   BACKSPACE clear\n",
            cursor.label.label()
        ),
        MUTED,
    ));
    detail.push((
        "Z ship   R recall beacon   H deploy beacon   J jump   G closes\n".into(),
        MUTED,
    ));

    detail.truncate(CHART_DETAIL);
    detail
}
