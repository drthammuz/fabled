//! TAB minimap: top-down overview of the current stretch grid.
//! Press TAB to toggle. Each cell = coloured square; connection slots = small
//! bars on the cell edges. Start = white, extraction = green, open = blue-grey,
//! sewer tunnel = teal, sewer double = olive, sewer cross = gold.
//!
//! Orientation: forward (+Z) = up on map, +X = left on map (matches in-game
//! perspective where east is to the player's left when facing north).

use bevy::prelude::*;
use shared::level::{ConnType, GridCell, RoomKind};
use shared::run::RunState;

use crate::level_render::LastRenderedLevel;
use crate::netplay::OwnPlayer;

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MinimapState>()
            .add_systems(Update, (toggle_tab, rebuild_if_stale, update_player_dot).chain());
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct MinimapState {
    visible: bool,
    built_id: String,
    built_seed: u64,
}

#[derive(Component)]
struct MinimapRoot;

#[derive(Component)]
struct MinimapCellNode;

#[derive(Component)]
struct MinimapPlayerDot;

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn toggle_tab(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<MinimapState>,
    mut roots: Query<&mut Visibility, With<MinimapRoot>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        state.visible = !state.visible;
        for mut vis in &mut roots {
            *vis = if state.visible { Visibility::Visible } else { Visibility::Hidden };
        }
    }
}

fn rebuild_if_stale(
    run: Query<&RunState>,
    last: Res<LastRenderedLevel>,
    mut state: ResMut<MinimapState>,
    mut commands: Commands,
    old_roots: Query<Entity, With<MinimapRoot>>,
) {
    let (level_id, seed) = run.single()
        .map(|s| (s.level_id.clone(), s.run_seed))
        .unwrap_or_else(|_| (String::new(), 0));

    if state.built_id == level_id && state.built_seed == seed { return; }
    if last.id != level_id { return; }

    state.built_id   = level_id.clone();
    state.built_seed = seed;

    for e in &old_roots { commands.entity(e).despawn(); }

    let level_def = shared::level::level_by_id(&level_id, seed);
    spawn_minimap(&mut commands, &level_def.grid_cells, state.visible);
}

/// Move the player dot to whichever grid cell the local player occupies.
fn update_player_dot(
    player: Query<&Transform, With<OwnPlayer>>,
    mut dot: Query<&mut Node, With<MinimapPlayerDot>>,
) {
    let Ok(tf) = player.single() else { return };
    let Ok(mut node) = dot.single_mut() else { return };
    const GRID: f32 = 12.0;
    let gx = (tf.translation.x / GRID).round() as i32;
    let gz = (tf.translation.z / GRID).round() as i32;
    node.left = Val::Px(cell_px(gx));
    node.top  = Val::Px(cell_py(gz));
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

const CELL_PX:  f32 = 22.0;  // each grid cell in pixels
const DOOR_PX:  f32 = 5.0;   // connection bar thickness
const PAD:      f32 = 6.0;   // container padding
// 5x5 grid + padding: 5*22 + 12 = 122px
const MAP_SIZE: f32 = 110.0;

/// Map grid X to screen-left pixels (+gx = left on screen, X axis flipped).
fn cell_px(gx: i32) -> f32 { PAD + MAP_SIZE * 0.5 - gx as f32 * CELL_PX - CELL_PX * 0.5 }
/// Map grid Z to screen-top pixels (+gz = up on screen = smaller top value).
fn cell_py(gz: i32) -> f32 { PAD + MAP_SIZE * 0.5 - gz as f32 * CELL_PX - CELL_PX * 0.5 }

fn cell_color(cell: &GridCell) -> Color {
    if cell.is_start      { return Color::srgba(0.95, 0.95, 0.95, 0.92); }
    if cell.is_extraction { return Color::srgba(0.10, 0.95, 0.30, 0.92); }
    match cell.room {
        RoomKind::Open        => Color::srgba(0.30, 0.40, 0.70, 0.82),
        RoomKind::SewerTunnel => Color::srgba(0.10, 0.55, 0.50, 0.82),
        RoomKind::SewerDouble => Color::srgba(0.20, 0.50, 0.35, 0.82),
        RoomKind::SewerCross  => Color::srgba(0.65, 0.55, 0.10, 0.82),
    }
}

fn conn_bar_color(c: ConnType) -> Option<Color> {
    match c {
        ConnType::None    => None,
        ConnType::BigArch => Some(Color::srgba(0.55, 0.70, 1.00, 0.90)),
        ConnType::Door    => Some(Color::srgba(0.70, 0.70, 0.70, 0.90)),
        ConnType::Shaft | ConnType::ShaftLeft | ConnType::ShaftRight
                          => Some(Color::srgba(0.90, 0.85, 0.40, 0.90)),
        ConnType::Sewer   => Some(Color::srgba(0.20, 0.85, 0.65, 0.90)),
    }
}

fn spawn_minimap(commands: &mut Commands, cells: &[GridCell], visible: bool) {
    if cells.is_empty() { return; }

    let vis = if visible { Visibility::Visible } else { Visibility::Hidden };
    let c = cells.to_vec();

    commands.spawn((
        MinimapRoot,
        Node {
            position_type: PositionType::Absolute,
            right:  Val::Px(14.0),
            bottom: Val::Px(72.0),
            width:  Val::Px(MAP_SIZE + PAD * 2.0),
            height: Val::Px(MAP_SIZE + PAD * 2.0),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.02, 0.06, 0.78)),
        vis,
    )).with_children(|root| {
        // TAB hint
        root.spawn((
            Text::new("TAB"),
            TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgba(0.5, 0.7, 0.9, 0.6)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(2.0),
                right:  Val::Px(5.0),
                ..default()
            },
        ));

        // Player dot — transparent cell-sized frame; update_player_dot repositions it each frame.
        root.spawn((
            MinimapPlayerDot,
            Node {
                position_type: PositionType::Absolute,
                left:   Val::Px(cell_px(0)),
                top:    Val::Px(cell_py(0)),
                width:  Val::Px(CELL_PX - 1.0),
                height: Val::Px(CELL_PX - 1.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        )).with_children(|dot_root| {
            let dot = (CELL_PX - 1.0) * 0.3;
            let off = (CELL_PX - 1.0) * 0.35;
            dot_root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left:   Val::Px(off),
                    top:    Val::Px(off),
                    width:  Val::Px(dot),
                    height: Val::Px(dot),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 0.9, 0.2, 0.95)),
            ));
        });

        for cell in &c {
            // +Z = up on screen; +X = LEFT on screen (X axis flipped vs world)
            let px = cell_px(cell.gx);
            let py = cell_py(cell.gz);
            let color = cell_color(cell);

            let ports = cell.ports;
            root.spawn((
                MinimapCellNode,
                Node {
                    position_type: PositionType::Absolute,
                    left:   Val::Px(px),
                    top:    Val::Px(py),
                    width:  Val::Px(CELL_PX - 1.0),
                    height: Val::Px(CELL_PX - 1.0),
                    ..default()
                },
                BackgroundColor(color),
            )).with_children(|cell_root| {
                for (side, &port) in ports.iter().enumerate() {
                    let Some(bar_color) = conn_bar_color(port) else { continue };
                    let cs = CELL_PX - 1.0;
                    // Side 0=+Z -> top, 1=-Z -> bottom, 2=+X -> LEFT (flipped), 3=-X -> RIGHT
                    let bar_node = match side {
                        0 => Node { position_type: PositionType::Absolute, top: Val::Px(0.0),    left:  Val::Px(cs*0.3), width: Val::Px(cs*0.4),  height: Val::Px(DOOR_PX), ..default() },
                        1 => Node { position_type: PositionType::Absolute, bottom: Val::Px(0.0), left:  Val::Px(cs*0.3), width: Val::Px(cs*0.4),  height: Val::Px(DOOR_PX), ..default() },
                        2 => Node { position_type: PositionType::Absolute, left:  Val::Px(0.0),  top:   Val::Px(cs*0.3), width: Val::Px(DOOR_PX), height: Val::Px(cs*0.4),  ..default() },
                        _ => Node { position_type: PositionType::Absolute, right: Val::Px(0.0),  top:   Val::Px(cs*0.3), width: Val::Px(DOOR_PX), height: Val::Px(cs*0.4),  ..default() },
                    };
                    cell_root.spawn((bar_node, BackgroundColor(bar_color)));
                }
            });
        }
    });
}
