//! Toggleable in-engine floor overlay (press **F4**).
//!
//! Draws one coloured square per floor-mask cell from the patched playtest layout —
//! the single source of truth that both visuals and physics now read:
//!   * green  = solid floor (you should stand here)
//!   * red    = hole (you should fall through here)
//!   * yellow = the cell directly under the player
//!
//! Because floor open/closed comes from one mask, if the overlay disagrees with what you
//! see or feel in the G playtest, that is the bug to chase — not a visual/physics split.

use bevy::prelude::*;
use shared::kenney_layout::KenneyLayout;
use shared::level::MOD_H;
use shared::{map_pool, EditorMode, TestMapStyle, TestMode};

pub struct FloorDebugPlugin;

impl Plugin for FloorDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FloorDebug>()
            .add_systems(Update, (toggle_floor_debug, draw_floor_debug).chain());
    }
}

#[derive(Resource, Default)]
pub struct FloorDebug {
    pub on: bool,
    layout: Option<KenneyLayout>,
}

fn is_kenney(test: Option<&TestMode>) -> bool {
    test.is_some_and(|t| t.style == TestMapStyle::Kenney)
}

fn toggle_floor_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<FloorDebug>,
    editor: Option<Res<EditorMode>>,
    test: Option<Res<TestMode>>,
) {
    if !keys.just_pressed(KeyCode::F4) {
        return;
    }
    state.on = !state.on;
    if state.on && is_kenney(test.as_deref()) {
        state.layout = Some(map_pool::play_layout(editor.is_some()));
        let holes: usize = state
            .layout
            .as_ref()
            .map(|l| l.floors.values().flat_map(|m| m.cells.iter()).filter(|c| !**c).count())
            .unwrap_or(0);
        info!("floor debug overlay ON ({} hole cells across all floors)", holes);
    } else if !state.on {
        info!("floor debug overlay OFF");
    }
}

fn draw_floor_debug(
    state: Res<FloorDebug>,
    mut gizmos: Gizmos,
    players: Query<&GlobalTransform, With<crate::netplay::OwnPlayer>>,
) {
    if !state.on {
        return;
    }
    let Some(layout) = &state.layout else {
        return;
    };
    let cell = layout.grid_unit_m;
    let half = cell * 0.5 - 0.06;
    let player = players.iter().next().map(|gt| gt.translation());

    for (level, mask) in &layout.floors {
        let y = *level as f32 * MOD_H + 0.12;
        let x0 = mask.world_x0();
        let z0 = mask.world_z0();
        for iz in 0..mask.cells_z {
            for ix in 0..mask.cells_x {
                let cx = x0 + (ix as f32 + 0.5) * cell;
                let cz = z0 + (iz as f32 + 0.5) * cell;
                // Only draw cells near the player to keep the overlay readable.
                if let Some(p) = player {
                    if (p.x - cx).abs() > 40.0 || (p.z - cz).abs() > 40.0 || (p.y - y).abs() > 9.0 {
                        continue;
                    }
                }
                let solid = mask.get(ix, iz);
                let under_player = player.is_some_and(|p| {
                    (p.x - cx).abs() <= half + 0.06
                        && (p.z - cz).abs() <= half + 0.06
                        && (p.y - y).abs() < MOD_H * 0.6
                });
                let color = if under_player {
                    Color::srgb(1.0, 0.95, 0.2)
                } else if solid {
                    Color::srgb(0.2, 0.9, 0.35)
                } else {
                    Color::srgb(0.95, 0.2, 0.2)
                };
                draw_cell(&mut gizmos, cx, y, cz, half, color, !solid);
            }
        }
    }
}

fn draw_cell(gizmos: &mut Gizmos, cx: f32, y: f32, cz: f32, half: f32, color: Color, cross: bool) {
    let a = Vec3::new(cx - half, y, cz - half);
    let b = Vec3::new(cx + half, y, cz - half);
    let c = Vec3::new(cx + half, y, cz + half);
    let d = Vec3::new(cx - half, y, cz + half);
    gizmos.line(a, b, color);
    gizmos.line(b, c, color);
    gizmos.line(c, d, color);
    gizmos.line(d, a, color);
    if cross {
        // Mark holes with an X so they read clearly from any angle.
        gizmos.line(a, c, color);
        gizmos.line(b, d, color);
    }
}
