//! The Bevy side of sound: bakes the synthesized samples once, then plays whatever the
//! headless `Mixer` decides is worth hearing. No game rules live here.

use crate::Session;
use bevy::{
    audio::{
        AudioPlayer, AudioSource, GlobalVolume, PlaybackMode, PlaybackSettings, SpatialListener,
        Volume,
    },
    prelude::*,
};
use ssc::mixer::Mixer;
use ssc::synth::{self, Sound};
use std::collections::HashMap;

const MASTER: f32 = 1.0;

#[derive(Resource)]
pub struct Bank(HashMap<Sound, Handle<AudioSource>>);

/// Marks a sounding effect, so the mixer can see how crowded it is.
#[derive(Component)]
pub struct Voice;

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
