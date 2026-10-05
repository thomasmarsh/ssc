# Gameplay and progression roadmap

The universe and creatures are in place (see [UNIVERSE.md](UNIVERSE.md)). The next focus is the player's side of the game: what you fly, what you shoot, what you pick up, and how the challenge grows. Items below are proposals to evaluate by playing, except the "Today" section, which describes the code.

## Today

- **Ship:** one ship, arrow-key thrust, rotate and brake, mouse aim. Top speed 460; a fling can carry it faster and it bleeds off.
- **Weapon:** a single forward gun (one shot every 0.16 s, 26 damage, 1.7 s of flight). No ammo, heat or alternates. Defined in `control_player` in `rust/simulation.rs`.
- **Defense:** shield that recharges after a quiet interval, hull, three lives, a protected respawn near where you died. Score only; no currency, upgrades or pickups.
- **Rewards already in the data:** every species has a `bounty` gene (score on kill, tracks the creature's power), bases pay 500, and the diet gene already has "dust" that heals creatures. Debris and dust exist as simulation concepts but the player cannot collect anything.
- **Difficulty:** driven only by position. `QuadrantParams::danger` rises with distance from the origin, and the gene pools bias toward harder creatures with aggression, tech and danger. There are no waves, levels or goals. Quadrant (0,0) is the calm, fixed start.
- **HUD:** quadrant coordinates, its six parameters, hostiles nearby, nearby species names, hull, shield, lives, TETHERED warning and a radar.

## Ideas to explore

**Weapons.** A small set of distinct tools beats many numeric upgrades. Candidates: spread, rapid fire, homing, piercing beam, mines, and a cutter for severing tethers. The creature genome already has projectile and tether weapons, so player weapons could share the same shot and hardpoint machinery. Open questions: unlimited base gun plus swappable secondary, or pickups that replace the gun; ammo or heat.

**Powerups and drops.** Use the bounty gene and ecology: creatures drop things according to what they are. Candidates: shield, hull, weapon charges, temporary speed, magnet or tractor (bases already tractor rocks), a brief lunatic-style negative mass field. Dust and debris could become collectables. Consider drops that exploit emergent mechanics (flinging a rock, luring a flock), in keeping with the game's mild humor.

**Graded difficulty.** The universe is already a difficulty gradient. Decide whether progress is spatial (go farther for harder) or has structure on top, such as named routes, landmark quadrants, or bosses (a base guarded by heavies is a natural one). Tame the far wilderness: tune `QUADRANT_BODY_BUDGET` and the pool biases so extreme quadrants are dangerous but not simply unplayable. Consider a soft difficulty curve that follows the player's strength as well as distance.

**Progression.** Options: pure arcade (score, lives, no persistence), per-run upgrades found by exploring, or meta-progression across runs. A seeded universe makes shareable routes and "this route has a good weapon at (x,y)" natural. Respawn rules interact with all of this (today you respawn where you died, with the world's kills persisting).

**Feedback and feel.** The prototype has no audio. Hit feedback, screen shake, pickup pop, and the C++ game's subtle absurdity (named species already give some of this) will matter more than numbers.

## Constraints from the generator

- New spawnable things (pickups, drops) must not disturb quadrant (0,0)'s golden test or the original RNG stream; give them their own salted stream, or derive them from kills at runtime.
- Persistence is by stable spawn index per quadrant; kills persist, positions and damage do not. Pickups need the same treatment or they will respawn on reload.
- Gameplay state lives in the headless simulation and is tested there; the renderer only draws it. Keep new systems deterministic.
- Player balance changes ripple into creature tuning (hull, damage and fire rates in genomes), so retune together.
