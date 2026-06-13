//! Full-screen class selection overlay shown before the first run.
//! Player presses 1–4 to pick a class; the screen disappears and the
//! cursor is locked for gameplay.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use shared::classes::ALL_CLASSES;
use shared::protocol::ClassPick;

pub struct ClassSelectPlugin;

/// Controls whether the class-select overlay is shown.
/// Starts as `Choosing`; transitions to `Playing` on class pick.
#[derive(States, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum SelectState {
    #[default]
    Choosing,
    Playing,
}

impl Plugin for ClassSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<SelectState>()
            .add_systems(Startup, setup_select_screen)
            .add_systems(
                Update,
                pick_class.run_if(in_state(SelectState::Choosing)),
            )
            .add_systems(OnEnter(SelectState::Playing), (hide_select_screen, lock_cursor));
    }
}

#[derive(Component)]
struct ClassSelectRoot;

fn setup_select_screen(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            ClassSelectRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(32.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.92)),
            ZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("SELECT YOUR CLASS"),
                TextFont { font_size: 32.0, ..default() },
                TextColor(Color::srgb(0.8, 0.9, 1.0)),
            ));

            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(24.0),
                ..default()
            })
            .with_children(|row| {
                for (i, def) in ALL_CLASSES.iter().enumerate() {
                    let [r, g, b] = def.capsule_color;
                    let accent = Color::srgb(r, g, b);
                    let portrait: Handle<Image> = asset_server.load(def.skin_path);

                    row.spawn((
                        Node {
                            width: Val::Px(200.0),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(16.0)),
                            row_gap: Val::Px(10.0),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.06, 0.1, 0.9)),
                        BorderColor::all(accent),
                    ))
                    .with_children(|card| {
                        // Portrait
                        card.spawn((
                            Node {
                                width: Val::Px(150.0),
                                height: Val::Px(150.0),
                                ..default()
                            },
                            ImageNode::new(portrait),
                        ));

                        // Key + name
                        card.spawn((
                            Text::new(format!("[{}] {}", i + 1, def.name)),
                            TextFont { font_size: 20.0, ..default() },
                            TextColor(accent),
                        ));

                        // Description
                        card.spawn((
                            Text::new(def.description),
                            TextFont { font_size: 12.5, ..default() },
                            TextColor(Color::srgba(0.75, 0.85, 0.9, 0.9)),
                        ));
                    });
                }
            });

            root.spawn((
                Text::new("Press 1 / 2 / 3 / 4 to choose"),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgba(0.6, 0.7, 0.8, 0.8)),
            ));
        });
}

fn pick_class(
    keys: Res<ButtonInput<KeyCode>>,
    mut writer: MessageWriter<ClassPick>,
    mut next: ResMut<NextState<SelectState>>,
) {
    let kind = if keys.just_pressed(KeyCode::Digit1) {
        shared::classes::ClassKind::Soldier
    } else if keys.just_pressed(KeyCode::Digit2) {
        shared::classes::ClassKind::Medic
    } else if keys.just_pressed(KeyCode::Digit3) {
        shared::classes::ClassKind::Scout
    } else if keys.just_pressed(KeyCode::Digit4) {
        shared::classes::ClassKind::Tech
    } else {
        return;
    };
    writer.write(ClassPick(kind));
    next.set(SelectState::Playing);
}

fn hide_select_screen(
    mut commands: Commands,
    screen: Query<Entity, With<ClassSelectRoot>>,
) {
    for entity in &screen {
        commands.entity(entity).despawn();
    }
}

fn lock_cursor(mut window: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    window.grab_mode = CursorGrabMode::Locked;
    window.visible = false;
}
