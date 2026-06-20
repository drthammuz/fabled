//! Client-side audio: title music, sewer ambient, electric buzz, footsteps,
//! and server-synchronised train sound.
//!
//! Required assets (place in assets/audio/):
//!   titlemusic.ogg          — played on the class-select screen
//!   sewer_ambient.ogg       — looped during stretch phases
//!   electricbuzz.ogg        — proximity buzz near warning lights
//!   train_random.ogg        — one-shot train sound, server-triggered
//!   step_walk.ogg           — footstep (walk and sprint, same sound)

use bevy::prelude::*;
use shared::protocol::{PlayerGrounded, PlayTrainSound};
use shared::run::RunPhase;

use crate::class_select::SelectState;
use crate::level_render::WarningLightPositions;
use crate::netplay::OwnPlayer;

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioHandles>()
            .init_resource::<FootstepState>()
            .add_systems(Startup, load_audio_handles)
            // Title music: start on class-select screen, stop when playing.
            .add_systems(OnEnter(SelectState::Choosing), start_title_music)
            .add_systems(OnEnter(SelectState::Playing), stop_title_music)
            // Manage sewer ambient based on run phase.
            .add_systems(Update, (
                manage_sewer_ambient,
                update_electric_buzz,
                handle_train_sound,
                own_player_footsteps,
            ));
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct AudioHandles {
    step: Option<Handle<AudioSource>>,
    title: Option<Handle<AudioSource>>,
    sewer: Option<Handle<AudioSource>>,
    buzz: Option<Handle<AudioSource>>,
    train: Option<Handle<AudioSource>>,
}

#[derive(Resource, Default)]
struct FootstepState {
    timer: f32,
}

// ---------------------------------------------------------------------------
// Marker components for managed audio entities
// ---------------------------------------------------------------------------

#[derive(Component)]
struct TitleMusicMarker;

#[derive(Component)]
struct SewerAmbientMarker;

#[derive(Component)]
struct ElectricBuzzMarker;

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

fn load_audio_handles(asset_server: Res<AssetServer>, mut handles: ResMut<AudioHandles>) {
    handles.step  = Some(asset_server.load("audio/step_walk.ogg"));
    handles.title = Some(asset_server.load("audio/titlemusic.ogg"));
    handles.sewer = Some(asset_server.load("audio/sewer_ambient.ogg"));
    handles.buzz  = Some(asset_server.load("audio/electricbuzz.ogg"));
    handles.train = Some(asset_server.load("audio/train_random.ogg"));
}

// ---------------------------------------------------------------------------
// Title music
// ---------------------------------------------------------------------------

fn start_title_music(mut commands: Commands, handles: Res<AudioHandles>) {
    let Some(h) = handles.title.clone() else { return };
    commands.spawn((
        TitleMusicMarker,
        AudioPlayer::<AudioSource>(h),
        PlaybackSettings {
            mode: bevy::audio::PlaybackMode::Loop,
            volume: bevy::audio::Volume::Linear(0.55),
            ..default()
        },
    ));
}

fn stop_title_music(
    mut commands: Commands,
    music: Query<Entity, With<TitleMusicMarker>>,
) {
    for e in &music {
        commands.entity(e).despawn();
    }
}

// ---------------------------------------------------------------------------
// Sewer ambient (InStretch only)
// ---------------------------------------------------------------------------

fn manage_sewer_ambient(
    mut commands: Commands,
    handles: Res<AudioHandles>,
    run: Query<&shared::run::RunState>,
    ambient: Query<Entity, With<SewerAmbientMarker>>,
) {
    let phase = run.single()
        .map(|s| s.phase)
        .unwrap_or(RunPhase::InStretch);

    let playing = !ambient.is_empty();
    let want = phase == RunPhase::InStretch;

    if want && !playing {
        let Some(h) = handles.sewer.clone() else { return };
        commands.spawn((
            SewerAmbientMarker,
            AudioPlayer::<AudioSource>(h),
            PlaybackSettings {
                mode: bevy::audio::PlaybackMode::Loop,
                volume: bevy::audio::Volume::Linear(0.70),
                ..default()
            },
        ));
    } else if !want && playing {
        for e in &ambient {
            commands.entity(e).despawn();
        }
    }
}

// ---------------------------------------------------------------------------
// Electric buzz — volume scales with proximity to warning lights
// ---------------------------------------------------------------------------

fn update_electric_buzz(
    mut commands: Commands,
    handles: Res<AudioHandles>,
    lights: Res<WarningLightPositions>,
    player: Query<&Transform, With<OwnPlayer>>,
    mut buzz_entity: Query<(Entity, &mut AudioSink), With<ElectricBuzzMarker>>,
) {
    // No warning lights → nothing to do.
    if lights.0.is_empty() {
        for (e, _) in &buzz_entity {
            commands.entity(e).despawn();
        }
        return;
    }

    let Ok(transform) = player.single() else { return };
    let pos = transform.translation;

    let min_dist_sq = lights.0.iter()
        .map(|lp| pos.distance_squared(*lp))
        .fold(f32::MAX, f32::min);
    let min_dist = min_dist_sq.sqrt();

    // Full volume within 3 m, fades to 0 at 12 m.
    const INNER: f32 = 3.0;
    const OUTER: f32 = 12.0;
    let volume = if min_dist < INNER {
        1.0_f32
    } else if min_dist > OUTER {
        0.0
    } else {
        1.0 - (min_dist - INNER) / (OUTER - INNER)
    };

    if let Ok((_, mut sink)) = buzz_entity.single_mut() {
        sink.set_volume(bevy::audio::Volume::Linear(volume * 0.45));
    } else if volume > 0.01 {
        // Spawn the looping buzz entity.
        let Some(h) = handles.buzz.clone() else { return };
        commands.spawn((
            ElectricBuzzMarker,
            AudioPlayer::<AudioSource>(h),
            PlaybackSettings {
                mode: bevy::audio::PlaybackMode::Loop,
                volume: bevy::audio::Volume::Linear(volume * 0.45),
                ..default()
            },
        ));
    }
}

// ---------------------------------------------------------------------------
// Train sound — server-synchronised one-shot
// ---------------------------------------------------------------------------

fn handle_train_sound(
    mut commands: Commands,
    handles: Res<AudioHandles>,
    mut messages: MessageReader<PlayTrainSound>,
) {
    for _ in messages.read() {
        let Some(h) = handles.train.clone() else { continue };
        commands.spawn((
            AudioPlayer::<AudioSource>(h),
            PlaybackSettings::DESPAWN,
        ));
    }
}

// ---------------------------------------------------------------------------
// Own-player footsteps — physics-grounded, no sound while airborne
// ---------------------------------------------------------------------------

fn own_player_footsteps(
    time:      Res<Time>,
    keys:      Res<ButtonInput<KeyCode>>,
    handles:   Res<AudioHandles>,
    mut state: ResMut<FootstepState>,
    mut commands: Commands,
    player:    Query<(&Transform, Option<&PlayerGrounded>), With<OwnPlayer>>,
) {
    let Ok((transform, grounded)) = player.single() else { return };

    // Use replicated physics grounded state; treat absent as not grounded.
    if !grounded.map(|g| g.0).unwrap_or(false) {
        state.timer = 0.0;
        return;
    }

    let dt = time.delta_secs().max(0.001);
    state.timer = (state.timer - dt).max(0.0);
    if state.timer > 0.0 { return; }

    // Estimate horizontal speed from position delta.
    // We still need speed to avoid playing footsteps while standing still.
    // A cheap proxy: check if any movement key is held.
    let moving = keys.pressed(KeyCode::KeyW)
        || keys.pressed(KeyCode::KeyS)
        || keys.pressed(KeyCode::KeyA)
        || keys.pressed(KeyCode::KeyD);
    if !moving { return; }

    let crouching = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if crouching {
        state.timer = 0.58; // silent crouch — still reset timer
        return;
    }

    let sprinting = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    state.timer = if sprinting { 0.30 } else { 0.46 };

    if let Some(h) = &handles.step {
        commands.spawn((
            AudioPlayer::<AudioSource>(h.clone()),
            PlaybackSettings {
                volume: bevy::audio::Volume::Linear(0.5),
                ..PlaybackSettings::DESPAWN
            },
        ));
    }

    // Suppress unused warning on transform — it's kept in the query
    // so Bevy ECS can track position changes.
    let _ = transform;
}
