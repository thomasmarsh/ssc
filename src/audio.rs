//! The Bevy side of sound: bakes the synthesized samples once, then plays whatever the
//! headless `Mixer` decides is worth hearing. No game rules live here.

use crate::Session;
use bevy::{
    audio::{
        AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, GlobalVolume, PlaybackMode,
        PlaybackSettings, SpatialListener, Volume,
    },
    prelude::*,
};
use ssc::backdrop::{self, Motif};
use ssc::mixer::{self, Mixer};
use ssc::realm::RealmKind;
use ssc::synth::{self, Sound};
use std::collections::HashMap;

const MASTER: f32 = 1.0;

#[derive(Resource)]
pub struct Bank(HashMap<Sound, Handle<AudioSource>>);

/// Marks a sounding effect, so the mixer can see how crowded it is.
#[derive(Component)]
pub struct Voice;

/// One realm kind's looping ambience; its level follows the realm's presence at the ship.
#[derive(Component)]
pub struct Bed(Motif);

#[derive(Resource, Default)]
pub struct Audio {
    mixer: Mixer,
    pub muted: bool,
}

pub fn setup(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let bank = Sound::ALL
        .into_iter()
        .map(|sound| {
            let bytes = synth::wav(&sound.render());
            (
                sound,
                sources.add(AudioSource {
                    bytes: bytes.into(),
                }),
            )
        })
        .collect();
    commands.insert_resource(Bank(bank));
    // Emitters are placed on a unit circle around the listener purely to pan them;
    // distance falloff is the mixer's job.
    commands.spawn((SpatialListener::new(0.2), Transform::default()));
    // The realm beds: one silent loop per kind, raised and lowered as the ship crosses the
    // border gradient (`play_beds`).
    for kind in RealmKind::all() {
        let motif = backdrop::motif_of(kind);
        let samples = mixer::bed_samples(motif);
        if samples.is_empty() {
            continue;
        }
        let handle = sources.add(AudioSource {
            bytes: synth::wav(&samples).into(),
        });
        commands.spawn((
            Bed(motif),
            AudioPlayer::new(handle),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(0.0),
                ..default()
            },
        ));
    }
}

/// Eases each realm bed toward the level its presence asks: continuous in the realm's
/// intensity, so the ambience arrives gradually and leaves as the way back is taken.
pub fn play_beds(session: Res<Session>, time: Res<Time>, mut beds: Query<(&Bed, &mut AudioSink)>) {
    let sky = backdrop::backdrop_at(session.game.seed(), session.game.focus);
    let motif = backdrop::motif_of(sky.realm);
    let step = (time.delta_secs() * 0.8).min(1.0);
    for (Bed(own), mut sink) in &mut beds {
        let target = if *own == motif {
            mixer::bed_gain(*own, sky.realm_ramp)
        } else {
            0.0
        };
        let now = match sink.volume() {
            Volume::Linear(v) => v,
            other => other.to_linear(),
        };
        sink.set_volume(Volume::Linear(now + (target - now) * step));
    }
}

pub fn apply_mute(audio: Res<Audio>, mut global: ResMut<GlobalVolume>) {
    let volume = if audio.muted { 0.0 } else { MASTER };
    global.volume = Volume::Linear(volume);
}

pub fn play_cues(
    mut commands: Commands,
    mut session: ResMut<Session>,
    mut audio: ResMut<Audio>,
    bank: Res<Bank>,
    time: Res<Time>,
    voices: Query<(), With<Voice>>,
) {
    let cues = session.game.drain_cues();
    if cues.is_empty() {
        return;
    }
    let listener = session
        .game
        .player()
        .map_or(session.game.focus, |p| p.position);
    let plays = audio
        .mixer
        .plan(time.elapsed_secs(), voices.count(), listener, &cues);
    for play in plays {
        let Some(handle) = bank.0.get(&play.sound) else {
            continue;
        };
        let pan = play.pan.clamp(-1.0, 1.0);
        commands.spawn((
            Voice,
            AudioPlayer::new(handle.clone()),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::Linear(play.gain),
                speed: play.speed,
                spatial: true,
                ..default()
            },
            Transform::from_xyz(pan, (1.0 - pan * pan).sqrt(), 0.0),
        ));
    }
}
