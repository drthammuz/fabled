//! Villager labels: a nameplate ("Edwin · tavern keeper") and an action
//! line ("→ tavern", "sleeping", ...) projected over each villager's head.
//! T cycles the action labels: nearby (default) → all → off. Nameplates
//! always show within close range. Also drives the sun from village time.

use bevy::prelude::*;
use shared::protocol::{VillageClock, Villager, VillagerState, VillagerStats};

use crate::fly_camera::FlyCamera;

pub struct VillagerUiPlugin;

impl Plugin for VillagerUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LabelMode>()
            .add_systems(Startup, spawn_clock_display)
            .add_systems(
                Update,
                (
                    toggle_label_mode,
                    spawn_labels,
                    update_labels,
                    cleanup_labels,
                    update_clock_display,
                    drive_sun,
                ),
            );
    }
}

/// How far away action labels are visible in `Nearby` mode (meters).
const NEARBY_RANGE: f32 = 28.0;
/// Nameplate visibility range (always on within this distance).
const NAME_RANGE: f32 = 18.0;

#[derive(Resource, Default, Clone, Copy, PartialEq)]
enum LabelMode {
    #[default]
    Nearby,
    All,
    Off,
}

#[derive(Component)]
struct VillagerLabel {
    target: Entity,
    name_line: Entity,
    action_line: Entity,
    stats_line: Entity,
}

fn toggle_label_mode(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<LabelMode>) {
    if keys.just_pressed(KeyCode::KeyT) {
        *mode = match *mode {
            LabelMode::Nearby => LabelMode::All,
            LabelMode::All => LabelMode::Off,
            LabelMode::Off => LabelMode::Nearby,
        };
        info!(
            "villager labels: {}",
            match *mode {
                LabelMode::Nearby => "nearby",
                LabelMode::All => "all",
                LabelMode::Off => "off",
            }
        );
    }
}

fn spawn_labels(mut commands: Commands, villagers: Query<(Entity, &Villager), Added<Villager>>) {
    for (entity, villager) in &villagers {
        let name_line = commands
            .spawn((
                Text::new(format!(
                    "{} · {}",
                    villager.name,
                    villager.profession.replace('_', " ")
                )),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ))
            .id();
        let action_line = commands
            .spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.85, 0.4)),
            ))
            .id();
        let stats_line = commands
            .spawn((
                Text::new(""),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.75, 0.85, 0.95)),
            ))
            .id();
        let label = commands
            .spawn((
                VillagerLabel {
                    target: entity,
                    name_line,
                    action_line,
                    stats_line,
                },
                Node {
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                    border_radius: BorderRadius::all(Val::Px(5.0)),
                    ..default()
                },
                // Readable over any backdrop.
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            ))
            .id();
        commands
            .entity(label)
            .add_children(&[name_line, action_line, stats_line]);
    }
}

fn action_text(state: &VillagerState) -> String {
    if state.walking {
        return format!("> walking to {}", state.place);
    }
    match state.action.as_str() {
        "sleep" => "zZz sleeping".to_string(),
        "eat" => "eating".to_string(),
        "work" => format!("working ({})", state.place),
        "warm_up" => "warming by the fire".to_string(),
        "socialize" => {
            if state.place == "tavern" {
                "having an ale".to_string()
            } else {
                "chatting on the square".to_string()
            }
        }
        "stroll" => "out for a stroll".to_string(),
        other => other.to_string(),
    }
}

fn update_labels(
    mode: Res<LabelMode>,
    camera: Single<(&Camera, &GlobalTransform), With<FlyCamera>>,
    villagers: Query<(&GlobalTransform, &VillagerState, &VillagerStats), With<Villager>>,
    mut labels: Query<(&VillagerLabel, &mut Node, &mut Visibility)>,
    mut texts: Query<&mut Text>,
    mut text_visibility: Query<&mut Visibility, Without<VillagerLabel>>,
) {
    let (camera, cam_transform) = *camera;
    let cam_pos = cam_transform.translation();
    for (label, mut node, mut visibility) in &mut labels {
        let Ok((transform, state, stats)) = villagers.get(label.target) else {
            continue;
        };
        // The models are ~1.7 m tall; hover the panel just over the head.
        let head = transform.translation() + Vec3::Y * 1.95;
        let distance = cam_pos.distance(head);

        let show_name = distance <= NAME_RANGE || *mode == LabelMode::All;
        let show_action = match *mode {
            LabelMode::Nearby => distance <= NEARBY_RANGE,
            LabelMode::All => true,
            LabelMode::Off => false,
        };
        if !show_name && !show_action {
            *visibility = Visibility::Hidden;
            continue;
        }
        match camera.world_to_viewport(cam_transform, head) {
            Ok(pos) => {
                node.left = Val::Px(pos.x - 60.0);
                node.top = Val::Px(pos.y - 30.0);
                *visibility = Visibility::Visible;
            }
            Err(_) => {
                *visibility = Visibility::Hidden;
                continue;
            }
        }
        if let Ok(mut name_vis) = text_visibility.get_mut(label.name_line) {
            *name_vis = if show_name {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        if let Ok(mut action_vis) = text_visibility.get_mut(label.action_line) {
            *action_vis = if show_action {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        if let Ok(mut stats_vis) = text_visibility.get_mut(label.stats_line) {
            *stats_vis = if show_action {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        if show_action {
            if let Ok(mut text) = texts.get_mut(label.action_line) {
                let next = action_text(state);
                if text.0 != next {
                    text.0 = next;
                }
            }
            if let Ok(mut text) = texts.get_mut(label.stats_line) {
                let next = format!(
                    "hun {} \u{00b7} nrg {} \u{00b7} wrm {} \u{00b7} lon {} \u{00b7} mood {} \u{00b7} {}c",
                    stats.hunger, stats.energy, stats.warmth, stats.social, stats.mood, stats.purse
                );
                if text.0 != next {
                    text.0 = next;
                }
            }
        }
    }
}

#[derive(Component)]
struct ClockDisplay;

fn spawn_clock_display(mut commands: Commands) {
    commands.spawn((
        ClockDisplay,
        Text::new("Day - --:--"),
        TextFont {
            font_size: 22.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.92, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(12.0),
            ..default()
        },
    ));
}

fn update_clock_display(
    clock: Query<&VillageClock>,
    mut display: Query<&mut Text, With<ClockDisplay>>,
) {
    let Ok(clock) = clock.single() else { return };
    let Ok(mut text) = display.single_mut() else {
        return;
    };
    let next = format!(
        "Day {} \u{00b7} {:02}:{:02}",
        clock.day,
        clock.minute_of_day / 60,
        clock.minute_of_day % 60
    );
    if text.0 != next {
        text.0 = next;
    }
}

fn cleanup_labels(
    mut commands: Commands,
    labels: Query<(Entity, &VillagerLabel)>,
    villagers: Query<(), With<Villager>>,
) {
    for (entity, label) in &labels {
        if villagers.get(label.target).is_err() {
            commands.entity(entity).despawn();
        }
    }
}

/// Day/night: rotate the sun from the replicated village clock and dim the
/// light overnight.
fn drive_sun(
    clock: Query<&VillageClock>,
    mut sun: Query<(&mut Transform, &mut DirectionalLight)>,
) {
    let Ok(clock) = clock.single() else { return };
    let Ok((mut transform, mut light)) = sun.single_mut() else {
        return;
    };
    let day_fraction = clock.minute_of_day as f32 / 1440.0;
    // 06:00 sunrise, 18:00 sunset; the sun arcs east to west.
    let sun_angle = (day_fraction - 0.25) * std::f32::consts::TAU;
    transform.rotation =
        Quat::from_euler(EulerRot::YXZ, -0.5, -sun_angle.sin().max(0.05) - 0.35, 0.0);
    let daylight = (sun_angle.sin()).clamp(0.0, 1.0);
    light.illuminance = 800.0 + 9_500.0 * daylight;
}
