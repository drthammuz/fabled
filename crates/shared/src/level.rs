//! Levels are dumb data: a list of spawnable things. For now they are built
//! by Rust functions; later they can be parsed from a file (e.g. a
//! TrenchBroom .map) into the same `LevelDef` without touching gameplay code.

use bevy::prelude::*;
use serde::Deserialize;

use crate::props::PropShape;

/// One cell in the generated grid — for the TAB minimap.
#[derive(Clone, Copy)]
pub struct GridCell {
    pub gx: i32,
    pub gz: i32,
    pub room: RoomKind,
    pub ports: [ConnType; 4],
    pub is_start: bool,
    pub is_extraction: bool,
}

/// A complete level description. Gameplay code consumes this; it never
/// cares where it came from.
pub struct LevelDef {
    pub id: String,
    pub statics: Vec<StaticDef>,
    pub props: Vec<PropDef>,
    pub lights: Vec<LightDef>,
    pub item_spawns: Vec<Vec3>,
    pub player_spawns: Vec<Vec3>,
    /// World position players must reach to extract from a stretch.
    pub extraction: Option<Vec3>,
    /// Enemy spawn points (stretch levels only).
    pub enemy_spawns: Vec<Vec3>,
    /// Grid layout for the TAB minimap (empty for static hub levels).
    pub grid_cells: Vec<GridCell>,
}

/// A point light baked into the level (cyan neons, warning oranges, toxic greens).
pub struct LightDef {
    pub position: Vec3,
    pub color: Color,
    pub intensity: f32,
    pub range: f32,
}

/// A dynamic physics prop placed by the level.
pub struct PropDef {
    pub shape: PropShape,
    pub position: Vec3,
    /// Mass density in kg/m^3; mass is derived from the collider volume.
    pub density: f32,
}

/// What a static piece of geometry *is*, for cosmetics and debugging.
/// Physics treats all of them identically (static colliders).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticKind {
    Floor,
    Wall,
    Ramp,
    Platform,
    /// Village building walls (timber look).
    Building,
    /// Flat cosmetic patches (farm field); thin, walkable.
    Field,
    /// The cobbled village square.
    Square,
    /// Dock planks over the water line.
    Pier,
    /// Pitched roof slabs (rotated cuboids, tiled).
    Roof,
    /// Triangular gable walls closing the roof ends. Client renders these
    /// as prisms; the server spawns no collider for them (they sit above
    /// the walls, inside the roof).
    Gable,
    /// Cyberpunk sewer floor (wet concrete).
    SewerFloor,
    /// Cyberpunk sewer wall / ceiling.
    SewerWall,
    /// Emissive accent strip.
    Neon,
    /// Red door marker from wall_map.json (visual only).
    DoorMarker,
    /// Raised metal walkway (player path).
    SewerWalkway,
    /// Toxic water channel — client visuals + server skips collider.
    SewerWater,
    /// Ceiling pipe / conduit bundle (visual). Axis-aligned cylinder whose
    /// long axis is its longest `size` dimension.
    SewerPipe,
    /// An arbitrarily-oriented pipe segment (elbow joints, culvert mouths).
    /// `rotation` orients local +Z to the pipe axis; `size` = (d, d, length).
    SewerPipeBend,
    /// The dark recessed interior of a culvert mouth — a near-black unlit disc
    /// behind the grate that reads as an open hole. `rotation`/`size` follow the
    /// same convention as [`StaticKind::SewerPipeBend`]. No collider.
    PipeBore,
    /// An OPEN-ended pipe mouth (a tube with no end caps) you can see into — the
    /// culvert ring. Same `rotation`/`size` convention as `SewerPipeBend`
    /// (diameter, diameter, length). No collider.
    PipeRing,
    /// A smooth swept-tube quarter-bend + vertical stub (one gap-free mesh).
    /// `position` = bend start, `rotation` maps local +Z→from-dir and +Y→to-dir,
    /// `size` = (pipe_radius, arc_radius, stub_length). Client sweeps a circle
    /// along the arc. No collider (sits at the ceiling, out of reach).
    PipeElbow,
    /// Ceiling cross-brace (visual only — no collider).
    SewerBrace,
    /// Arch rib spanning the tunnel width (visual only — no collider).
    SewerArch,
    /// Low horizontal duct the player must crouch under.
    SewerDuct,
}

/// One static cuboid: full extents `size`, centered at `position`.
pub struct StaticDef {
    pub kind: StaticKind,
    pub position: Vec3,
    pub rotation: Quat,
    pub size: Vec3,
}

impl StaticDef {
    fn axis_aligned(kind: StaticKind, position: Vec3, size: Vec3) -> Self {
        Self {
            kind,
            position,
            rotation: Quat::IDENTITY,
            size,
        }
    }
}

/// Radius, length and rotation for an axis-aligned [`StaticKind::SewerPipe`]
/// cylinder. The pipe's long axis is its longest `size` dimension; the returned
/// rotation aligns a +Y-axis cylinder (Bevy mesh / Avian collider convention)
/// to that axis. Used by both the client (mesh) and server (collider) so the
/// visible pipe and its solid collider always match.
pub fn straight_pipe(def: &StaticDef) -> (f32, f32, Quat) {
    use std::f32::consts::FRAC_PI_2;
    let s = def.size;
    let radius = s.x.min(s.y).min(s.z) * 0.5;
    let length = s.x.max(s.y).max(s.z);
    let rot = if s.z >= s.x && s.z >= s.y {
        def.rotation * Quat::from_rotation_x(FRAC_PI_2)
    } else if s.x >= s.y {
        def.rotation * Quat::from_rotation_z(FRAC_PI_2)
    } else {
        def.rotation
    };
    (radius, length, rot)
}

/// Radius, length and rotation for a [`StaticKind::SewerPipeBend`] segment.
/// `size` is (d, d, length) and `rotation` orients local +Z to the pipe axis;
/// the returned rotation aligns a +Y-axis cylinder to that same axis.
pub fn bend_pipe(def: &StaticDef) -> (f32, f32, Quat) {
    let radius = def.size.x * 0.5;
    let length = def.size.z;
    let axis = def.rotation * Vec3::Z;
    (radius, length, Quat::from_rotation_arc(Vec3::Y, axis))
}

use std::sync::OnceLock;

use crate::TestMapStyle;

static TEST_MAP_STYLE: OnceLock<TestMapStyle> = OnceLock::new();

/// Set before the app runs (from `main` when `--test` is passed).
pub fn set_test_map_style(style: TestMapStyle) {
    let _ = TEST_MAP_STYLE.set(style);
}

pub fn test_map_style() -> TestMapStyle {
    TEST_MAP_STYLE.get().copied().unwrap_or(TestMapStyle::Rusty)
}

static EDITOR_LAYOUT: OnceLock<bool> = OnceLock::new();

/// Set before the app runs (from `main` when `--editor` is passed).
pub fn set_editor_layout(open: bool) {
    let _ = EDITOR_LAYOUT.set(open);
}

fn editor_layout() -> bool {
    EDITOR_LAYOUT.get().copied().unwrap_or(false)
}

/// The level every run mode currently loads (overridden by run state on server).
pub fn active_level() -> LevelDef {
    level_by_id("sewer_entry", 0)
}

/// Look up a level by id from the stretch graph, generating geometry from `seed`.
pub fn level_by_id(id: &str, seed: u64) -> LevelDef {
    if id == "testmap" {
        return testmap_level(seed);
    }
    if id == "city" {
        return city_level(seed);
    }
    crate::run::node(id)
        .map(|n| (n.build)(seed))
        .unwrap_or_else(|| sewer_entry_level(seed))
}

/// Empty level for `--city` — geometry + collision come from the city GLB.
pub fn city_level(_seed: u64) -> LevelDef {
    LevelDef {
        id: "city".into(),
        statics: vec![],
        props: vec![],
        lights: vec![],
        item_spawns: vec![],
        player_spawns: vec![crate::config::CITY_SPAWN],
        extraction: None,
        enemy_spawns: vec![],
        grid_cells: vec![],
    }
}

/// Developer test map — Kenney sandbox sized from saved layout (or 5×5 default).
pub fn testmap_level(_seed: u64) -> LevelDef {
    use StaticKind::*;
    let layout = crate::map_pool::test_play_layout();
    let (ex, ez) = layout.map_extent_m();
    let w = ex.max(ez).max(kenney_sandbox_extent_m());
    let (cx, cz) = if layout.pieces.is_empty() && layout.floors.is_empty() {
        kenney_sandbox_center_xz()
    } else {
        layout.map_center_xz()
    };

    let mut statics = Vec::new();
    // Fallback slab when no per-cell floors exported yet.
    if layout.floors.is_empty() {
        sa_v(
            &mut statics,
            SewerFloor,
            Vec3::new(cx, -0.1, cz),
            Vec3::new(w, 0.2, w),
        );
    }

    LevelDef {
        id: "testmap".into(),
        statics,
        props: vec![],
        lights: vec![],
        item_spawns: vec![],
        player_spawns: vec![
            Vec3::new(cx - 2.0, 1.2, cz - 6.0),
            Vec3::new(cx + 2.0, 1.2, cz - 6.0),
            Vec3::new(cx - 2.0, 1.2, cz - 4.0),
            Vec3::new(cx + 2.0, 1.2, cz - 4.0),
        ],
        // Hub floor at y = -MOD_H; flag points to hub interior (~standing height).
        extraction: layout
            .extraction_xz
            .map(|[x, z]| Vec3::new(x, -3.0, z)),
        enemy_spawns: vec![],
        grid_cells: vec![],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// User coordinate wall map — loaded from userinput/wall_map.json
// ─────────────────────────────────────────────────────────────────────────────

const WALL_MAP_JSON: &str = include_str!("../../../userinput/wall_map.json");

static WALL_MAP: std::sync::OnceLock<WallMapSpec> = std::sync::OnceLock::new();

#[derive(Deserialize)]
struct WallMapSpec {
    coord_system: WallCoordSystem,
    grid: WallGridBounds,
    outer_walls: Vec<WallSegment>,
    inner_walls: Vec<WallSegment>,
    #[serde(default)]
    doors: Vec<DoorSegment>,
    stairs: StairsSpec,
}

#[derive(Deserialize)]
struct DoorSegment {
    from: [f32; 2],
    to: [f32; 2],
}

#[derive(Deserialize)]
struct WallCoordSystem {
    world_x0: f32,
    cell_meters: f32,
}

#[derive(Deserialize)]
struct WallGridBounds {
    max_x: f32,
    max_y: f32,
}

#[derive(Deserialize)]
struct WallSegment {
    from: [f32; 2],
    to: [f32; 2],
}

#[derive(Deserialize)]
struct ClimbSpec {
    from: [f32; 2],
    to: [f32; 2],
}

#[derive(Deserialize)]
struct StairsSpec {
    corners: [[f32; 2]; 4],
    /// Low end → high end in grid coords. Corner order is ignored (only the
    /// bounding box matters); use this to set which way the ramp faces.
    #[serde(default)]
    climb: Option<ClimbSpec>,
}

fn wall_map_spec() -> &'static WallMapSpec {
    WALL_MAP.get_or_init(|| serde_json::from_str(WALL_MAP_JSON).expect("userinput/wall_map.json must be valid"))
}

/// West edge of user x=0 in world space (from wall_map.json).
/// Editor / `--test --kenney` playfield: 3×3 Kenney modules (60 m), default map size.
pub const KENNEY_SANDBOX_CELLS: f32 = 15.0;

pub fn kenney_sandbox_extent_m() -> f32 {
    KENNEY_SANDBOX_CELLS * user_map_cell()
}

pub fn kenney_sandbox_center_xz() -> (f32, f32) {
    let w = kenney_sandbox_extent_m();
    (user_map_x0() + w * 0.5, w * 0.5)
}

pub fn user_map_x0() -> f32 { wall_map_spec().coord_system.world_x0 }

/// One grid unit in metres (from wall_map.json).
pub fn user_map_cell() -> f32 { wall_map_spec().coord_system.cell_meters }

/// West edge of user x=0 in world space.
pub const USER_MAP_X0: f32 = -16.0;

/// User grid coordinate → world X (east = +X).
pub fn user_x(x: f32) -> f32 { user_map_x0() + x * user_map_cell() }
/// User grid coordinate → world Z (north = +Z, y=0 is south).
pub fn user_z(y: f32) -> f32 { y * user_map_cell() }

const WALL_EPS: f32 = 0.001;

/// Shared-axis overlap between collinear door and wall; returns opening centre on that axis.
fn door_opening_on_wall(door: &DoorSegment, seg: &WallSegment) -> Option<f32> {
    let (wx0, wy0, wx1, wy1) = (seg.from[0], seg.from[1], seg.to[0], seg.to[1]);
    let (dx0, dy0, dx1, dy1) = (door.from[0], door.from[1], door.to[0], door.to[1]);
    if (wy0 - wy1).abs() < WALL_EPS && (dy0 - dy1).abs() < WALL_EPS && (wy0 - dy0).abs() < WALL_EPS {
        let w_lo = wx0.min(wx1);
        let w_hi = wx0.max(wx1);
        let d_lo = dx0.min(dx1);
        let d_hi = dx0.max(dx1);
        let o_lo = w_lo.max(d_lo);
        let o_hi = w_hi.min(d_hi);
        if o_hi > o_lo + WALL_EPS {
            return Some((o_lo + o_hi) * 0.5);
        }
        let cx = (dx0 + dx1) * 0.5;
        if cx >= w_lo - WALL_EPS && cx <= w_hi + WALL_EPS {
            return Some(cx);
        }
    } else if (wx0 - wx1).abs() < WALL_EPS && (dx0 - dx1).abs() < WALL_EPS && (wx0 - dx0).abs() < WALL_EPS {
        let w_lo = wy0.min(wy1);
        let w_hi = wy0.max(wy1);
        let d_lo = dy0.min(dy1);
        let d_hi = dy0.max(dy1);
        let o_lo = w_lo.max(d_lo);
        let o_hi = w_hi.min(d_hi);
        if o_hi > o_lo + WALL_EPS {
            return Some((o_lo + o_hi) * 0.5);
        }
        let cy = (dy0 + dy1) * 0.5;
        if cy >= w_lo - WALL_EPS && cy <= w_hi + WALL_EPS {
            return Some(cy);
        }
    }
    None
}

/// Opening centres along the segment axis (grid x for horizontal, grid y for vertical).
fn doors_on_segment(seg: &WallSegment, doors: &[DoorSegment]) -> Vec<f32> {
    let mut along: Vec<f32> = doors
        .iter()
        .filter_map(|d| door_opening_on_wall(d, seg))
        .collect();
    along.sort_by(|a, b| a.partial_cmp(b).unwrap());
    along.dedup_by(|a, b| (*a - *b).abs() < WALL_EPS);
    along
}

/// Place one wall segment given endpoints in user grid coords.
fn wall_line(
    s: &mut Vec<StaticDef>,
    x0: f32, y0: f32, x1: f32, y1: f32,
    y_lo: f32, y_hi: f32, wt: f32,
    opens_along: &[f32],
) {
    if (y0 - y1).abs() < WALL_EPS {
        let z = user_z(y0);
        let opens: Vec<f32> = opens_along.iter().map(|&gx| user_x(gx)).collect();
        wall_ns(s, user_x(x0.min(x1)), user_x(x0.max(x1)), z, y_lo, y_hi, wt, &opens);
    } else if (x0 - x1).abs() < WALL_EPS {
        let x = user_x(x0);
        let opens: Vec<f32> = opens_along.iter().map(|&gy| user_z(gy)).collect();
        wall_ew(s, user_z(y0.min(y1)), user_z(y0.max(y1)), x, y_lo, y_hi, wt, &opens);
    }
}

fn place_wall_segment(
    statics: &mut Vec<StaticDef>,
    seg: &WallSegment,
    doors: &[DoorSegment],
    y_lo: f32,
    y_hi: f32,
    wt: f32,
) {
    let opens = doors_on_segment(seg, doors);
    wall_line(
        statics,
        seg.from[0], seg.from[1], seg.to[0], seg.to[1],
        y_lo, y_hi, wt, &opens,
    );
}

fn user_wall_map(statics: &mut Vec<StaticDef>, lights: &mut Vec<LightDef>) {
    use StaticKind::*;
    let spec = wall_map_spec();
    let cell = spec.coord_system.cell_meters;
    const WT: f32 = 0.35;
    let y_lo = 0.0_f32;
    let y_hi = MOD_H;

    let floor_w = (spec.grid.max_x + 1.0) * cell;
    let floor_d = (spec.grid.max_y + 1.0) * cell;
    let floor_cx = user_map_x0() + floor_w * 0.5;
    let floor_cz = floor_d * 0.5;

    sa_v(statics, SewerFloor, Vec3::new(floor_cx, y_lo - 0.1, floor_cz), Vec3::new(floor_w, 0.2, floor_d));
    if !editor_layout() {
        sa_v(statics, SewerWall,  Vec3::new(floor_cx, y_hi + 0.1, floor_cz), Vec3::new(floor_w, 0.2, floor_d));
    }

    for seg in &spec.outer_walls {
        place_wall_segment(statics, seg, &spec.doors, y_lo, y_hi, WT);
    }
    for seg in &spec.inner_walls {
        place_wall_segment(statics, seg, &spec.doors, y_lo, y_hi, WT);
    }

    pt_v(lights, Vec3::new(0.0, y_hi - 0.9, floor_cz), Color::srgb(0.4, 0.7, 1.0), 55_000.0, 20.0);

    place_door_markers(statics, &spec.doors, cell);
    if test_map_style() == TestMapStyle::Rusty {
        place_stairs(statics, lights, &spec.stairs, cell, y_lo, y_hi);
    }
}

fn place_door_markers(statics: &mut Vec<StaticDef>, doors: &[DoorSegment], cell: f32) {
    use StaticKind::DoorMarker;
    const MARKER_H: f32 = 0.12;
    const MARKER_T: f32 = 0.14;
    let y = 0.2_f32;

    for door in doors {
        let (x0, y0, x1, y1) = (
            door.from[0], door.from[1], door.to[0], door.to[1],
        );
        if (y0 - y1).abs() < WALL_EPS {
            let len = (x1 - x0).abs() * cell;
            if len < 0.01 {
                continue;
            }
            sa_v(
                statics,
                DoorMarker,
                Vec3::new(user_x((x0 + x1) * 0.5), y, user_z(y0)),
                Vec3::new(len * 0.95, MARKER_H, MARKER_T),
            );
        } else if (x0 - x1).abs() < WALL_EPS {
            let len = (y1 - y0).abs() * cell;
            if len < 0.01 {
                continue;
            }
            sa_v(
                statics,
                DoorMarker,
                Vec3::new(user_x(x0), y, user_z((y0 + y1) * 0.5)),
                Vec3::new(MARKER_T, MARKER_H, len * 0.95),
            );
        }
    }
}

fn place_stairs(
    statics: &mut Vec<StaticDef>,
    lights: &mut Vec<LightDef>,
    stairs: &StairsSpec,
    cell: f32,
    y_lo: f32,
    y_hi: f32,
) {
    use StaticKind::*;
    let xs: [f32; 4] = stairs.corners.map(|c| c[0]);
    let ys: [f32; 4] = stairs.corners.map(|c| c[1]);
    let st_x0 = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let st_x1 = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let st_y0 = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let st_y1 = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let mid_x = (st_x0 + st_x1) * 0.5;
    let mid_y = (st_y0 + st_y1) * 0.5;
    let (climb_from, climb_to) = match &stairs.climb {
        Some(c) => (c.from, c.to),
        None => ([mid_x, st_y0], [mid_x, st_y1]),
    };

    let dx = climb_to[0] - climb_from[0];
    let dy = climb_to[1] - climb_from[1];
    let rise = y_hi - y_lo;
    let (width, run, position, rotation, ramp_size) = if dx.abs() >= dy.abs() {
        let width = (st_y1 - st_y0) * cell;
        let run = dx.abs() * cell;
        let angle = rise.atan2(run);
        let slope_len = (run * run + rise * rise).sqrt();
        let x_lo = climb_from[0].min(climb_to[0]);
        let rot = if dx > 0.0 {
            Quat::from_rotation_z(angle)
        } else {
            Quat::from_rotation_z(-angle)
        };
        (
            width,
            run,
            Vec3::new(
                user_x(x_lo) + run * 0.5,
                y_lo + rise * 0.5,
                user_z(mid_y),
            ),
            rot,
            Vec3::new(slope_len, 0.45, width * 0.92),
        )
    } else {
        let width = (st_x1 - st_x0) * cell;
        let run = dy.abs() * cell;
        let angle = rise.atan2(run);
        let slope_len = (run * run + rise * rise).sqrt();
        let z_lo = climb_from[1].min(climb_to[1]);
        let rot = if dy > 0.0 {
            Quat::from_rotation_x(angle)
        } else {
            Quat::from_rotation_x(-angle)
        };
        (
            width,
            run,
            Vec3::new(
                user_x(mid_x),
                y_lo + rise * 0.5,
                user_z(z_lo) + run * 0.5,
            ),
            rot,
            Vec3::new(width * 0.92, 0.45, slope_len),
        )
    };

    statics.push(StaticDef {
        kind: Ramp,
        position,
        rotation,
        size: ramp_size,
    });

    sa_v(
        statics,
        Neon,
        Vec3::new(
            user_x((st_x0 + st_x1) * 0.5),
            y_lo + 0.06,
            user_z((st_y0 + st_y1) * 0.5),
        ),
        Vec3::new(width * 0.85, 0.06, run * 0.85),
    );

    pt_v(
        lights,
        Vec3::new(
            user_x((st_x0 + st_x1) * 0.5),
            y_hi - 0.9,
            user_z((st_y0 + st_y1) * 0.5),
        ),
        Color::srgb(1.0, 0.85, 0.2),
        25_000.0,
        10.0,
    );
}

/// Kenney `stairs.glb` placement from `wall_map.json` (floor origin + yaw).
///
/// Model convention (yaw = 0): entry arch at local Z = −6.1, landing at +2.1.
pub fn kenney_stairs_placement() -> Option<(Vec3, f32)> {
    let entry_offset = crate::kenney_catalog::piece("stairs")
        .and_then(|p| p.stairs.as_ref())
        .map(|s| -s.entry_z)
        .unwrap_or(6.1);
    let spec = wall_map_spec();
    let stairs = &spec.stairs;
    let xs: [f32; 4] = stairs.corners.map(|c| c[0]);
    let ys: [f32; 4] = stairs.corners.map(|c| c[1]);
    let st_x0 = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let st_x1 = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let st_y0 = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let st_y1 = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mid_x = (st_x0 + st_x1) * 0.5;
    let (climb_from, climb_to) = match &stairs.climb {
        Some(c) => (c.from, c.to),
        None => ([mid_x, st_y0], [mid_x, st_y1]),
    };
    let dx = climb_to[0] - climb_from[0];
    let dy = climb_to[1] - climb_from[1];
    if dy.abs() < dx.abs() {
        return None;
    }
    let x = user_x(mid_x);
    if dy > 0.0 {
        Some((Vec3::new(x, 0.002, user_z(climb_from[1]) + entry_offset), 0.0))
    } else {
        Some((
            Vec3::new(x, 0.002, user_z(climb_from[1]) - entry_offset),
            std::f32::consts::PI,
        ))
    }
}

// Legacy example-grid helpers (kept for Kenney showcase constants).

/// One grid unit = 4 m.
pub const GRID_CELL: f32 = 4.0;
pub const GRID_COLS: i32 = 9;
pub const GRID_ROWS: i32 = 6;

/// West edge of vertical grid line `ci` (0..=9).
pub fn grid_x(ci: i32) -> f32 { GRID_X0 + ci as f32 * GRID_CELL }
/// South edge of cell row `ri` (0 = north).
pub fn grid_z_south(ri: i32) -> f32 { (GRID_ROWS - 1 - ri) as f32 * GRID_CELL }
/// North edge of cell row `ri`.
pub fn grid_z_north(ri: i32) -> f32 { (GRID_ROWS - ri) as f32 * GRID_CELL }
/// Centre X of cell column `ci`.
pub fn cell_x(ci: i32) -> f32 { grid_x(ci) + GRID_CELL * 0.5 }
/// Centre Z of cell row `ri`.
pub fn cell_z(ri: i32) -> f32 { grid_z_south(ri) + GRID_CELL * 0.5 }

/// Module footprint (12 m — Kenney kit compatibility).
pub const MOD_SIZE:   f32 = 12.0;
pub const MOD_H:      f32 = 4.5;
pub const MOD_OPEN_W: f32 = 4.0;
pub const MOD_OPEN_H: f32 = 4.25;
pub const GRID_X0: f32 = -18.0;
pub const GRID_Z0: f32 =   0.0;
pub const SLOT_L: f32 =  2.0;
pub const SLOT_C: f32 =  6.0;
pub const SLOT_R: f32 = 10.0;

/// Wall perpendicular to Z (spans x_lo..x_hi at fixed z, y_lo..y_hi).
/// `opens`: world x-positions of opening centres.
fn wall_ns(
    s: &mut Vec<StaticDef>,
    x_lo: f32, x_hi: f32,
    z: f32,
    y_lo: f32, y_hi: f32,
    t: f32,
    opens: &[f32],
) {
    use StaticKind::SewerWall;
    let wh     = y_hi - y_lo;
    let y_ctr  = (y_lo + y_hi) * 0.5;
    let hw     = MOD_OPEN_W * 0.5;
    let lintel = wh - MOD_OPEN_H;

    let mut ops: Vec<f32> = opens.to_vec();
    ops.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut cur = x_lo;
    for &ox in &ops {
        let end = ox - hw;
        if end > cur + 0.02 {
            sa_v(s, SewerWall,
                Vec3::new((cur + end) * 0.5, y_ctr, z),
                Vec3::new(end - cur, wh, t));
        }
        cur = ox + hw;
    }
    if cur < x_hi - 0.02 {
        sa_v(s, SewerWall,
            Vec3::new((cur + x_hi) * 0.5, y_ctr, z),
            Vec3::new(x_hi - cur, wh, t));
    }
    if lintel > 0.02 {
        let ly = y_lo + MOD_OPEN_H + lintel * 0.5;
        for &ox in &ops {
            sa_v(s, SewerWall, Vec3::new(ox, ly, z), Vec3::new(MOD_OPEN_W, lintel, t));
        }
    }
}

/// Wall perpendicular to X (spans z_lo..z_hi at fixed x, y_lo..y_hi).
/// `opens`: world z-positions of opening centres.
fn wall_ew(
    s: &mut Vec<StaticDef>,
    z_lo: f32, z_hi: f32,
    x: f32,
    y_lo: f32, y_hi: f32,
    t: f32,
    opens: &[f32],
) {
    use StaticKind::SewerWall;
    let wh     = y_hi - y_lo;
    let y_ctr  = (y_lo + y_hi) * 0.5;
    let hw     = MOD_OPEN_W * 0.5;
    let lintel = wh - MOD_OPEN_H;

    let mut ops: Vec<f32> = opens.to_vec();
    ops.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut cur = z_lo;
    for &oz in &ops {
        let end = oz - hw;
        if end > cur + 0.02 {
            sa_v(s, SewerWall,
                Vec3::new(x, y_ctr, (cur + end) * 0.5),
                Vec3::new(t, wh, end - cur));
        }
        cur = oz + hw;
    }
    if cur < z_hi - 0.02 {
        sa_v(s, SewerWall,
            Vec3::new(x, y_ctr, (cur + z_hi) * 0.5),
            Vec3::new(t, wh, z_hi - cur));
    }
    if lintel > 0.02 {
        let ly = y_lo + MOD_OPEN_H + lintel * 0.5;
        for &oz in &ops {
            sa_v(s, SewerWall, Vec3::new(x, ly, oz), Vec3::new(t, lintel, MOD_OPEN_W));
        }
    }
}

// ---------------------------------------------------------------------------
// Sewer procgen
// ---------------------------------------------------------------------------

/// Parameters for a generated sewer stretch.
pub struct StretchParams {
    pub id: String,
    /// Depth into the run (0 = entry, higher = harder, more enemies).
    pub depth: u32,
}

/// Fast deterministic xorshift64 RNG — no external deps.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        let mut s = Self(seed ^ 0x9e3779b97f4a7c15);
        s.next(); s.next();
        s
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f32(&mut self) -> f32 {
        (self.next() >> 11) as f32 * (1.0 / (1u64 << 53) as f32)
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f32() * (hi - lo)
    }
    fn coin(&mut self, prob: f32) -> bool {
        self.f32() < prob
    }
    fn umod(&mut self, n: usize) -> usize {
        self.next() as usize % n
    }
}

// ---------------------------------------------------------------------------
// Module-based labyrinth generator
// ---------------------------------------------------------------------------

const GRID:     f32 = 12.0;  // cell footprint (square)
const CELL_H:   f32 = 4.2;   // open-room interior height
const OPEN_W:   f32 = 4.0;   // BigArch opening width
const DOOR_W:   f32 = 1.2;   // door opening width
const DOOR_H:   f32 = 2.4;   // door opening height
const SHAFT_W:  f32 = 1.5;   // shaft opening (square)
const SEWER_W:  f32 = 2.5;   // sewer tunnel interior width
const SEWER_H:  f32 = 2.8;   // sewer tunnel interior height
const WALL_T:   f32 = 0.38;
const GHALF:    f32 = GRID / 2.0;
// Fill width = from tunnel edge to cell edge (may slightly overlap outer wall — intentional)
const SEWER_FILL: f32 = GHALF - SEWER_W * 0.5; // 4.75 m

/// How a module edge connects to its neighbour.
/// ShaftLeft/ShaftRight are shaft-sized openings offset OFFSET_SHIFT toward –axis / +axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnType { None, BigArch, Door, Shaft, ShaftLeft, ShaftRight, Sewer }

/// Interior geometry style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind { Open, SewerTunnel, SewerDouble, SewerCross }

struct MDef { room: RoomKind, ports: [ConnType; 4], weight: u32 }

// Side indices: 0=+Z  1=-Z  2=+X  3=-X
fn opp_side(s: usize) -> usize { s ^ 1 }

fn nb(gx: i32, gz: i32, side: usize) -> (i32, i32) {
    match side {
        0 => (gx, gz + 1),
        1 => (gx, gz - 1),
        2 => (gx + 1, gz),
        _ => (gx - 1, gz),
    }
}

/// Two connection types are compatible when placed on opposing faces of adjacent modules.
/// Only exact-type matches are allowed — cross-type transitions produce mismatched wall
/// geometry (different opening sizes in the same shared wall plane).
fn conn_compat(a: ConnType, b: ConnType) -> bool {
    a == b
}

fn module_defs() -> &'static [MDef] {
    use ConnType::*;
    use RoomKind::*;
    &[
        // ── Open rooms — BigArch (≥2 ports) ──
        MDef { room: Open, ports: [BigArch, BigArch, None,    None   ], weight: 8 },
        MDef { room: Open, ports: [None,    None,    BigArch, BigArch], weight: 8 },
        MDef { room: Open, ports: [BigArch, None,    BigArch, None   ], weight: 5 },
        MDef { room: Open, ports: [BigArch, None,    None,    BigArch], weight: 5 },
        MDef { room: Open, ports: [None,    BigArch, BigArch, None   ], weight: 5 },
        MDef { room: Open, ports: [None,    BigArch, None,    BigArch], weight: 5 },
        MDef { room: Open, ports: [BigArch, BigArch, BigArch, None   ], weight: 3 },
        MDef { room: Open, ports: [BigArch, BigArch, None,    BigArch], weight: 3 },
        MDef { room: Open, ports: [BigArch, None,    BigArch, BigArch], weight: 3 },
        MDef { room: Open, ports: [None,    BigArch, BigArch, BigArch], weight: 3 },
        MDef { room: Open, ports: [BigArch, BigArch, BigArch, BigArch], weight: 2 },
        // ── Open rooms — Door (narrower) ──
        MDef { room: Open, ports: [Door, Door, None, None], weight: 4 },
        MDef { room: Open, ports: [None, None, Door, Door], weight: 4 },
        MDef { room: Open, ports: [Door, None, Door, None], weight: 3 },
        MDef { room: Open, ports: [Door, None, None, Door], weight: 3 },
        MDef { room: Open, ports: [None, Door, Door, None], weight: 3 },
        MDef { room: Open, ports: [None, Door, None, Door], weight: 3 },
        // ── Gateway: bridges Open ↔ Sewer network ──
        MDef { room: Open, ports: [Sewer,   BigArch, None,    None   ], weight: 4 },
        MDef { room: Open, ports: [BigArch, Sewer,   None,    None   ], weight: 4 },
        MDef { room: Open, ports: [None,    None,    Sewer,   BigArch], weight: 4 },
        MDef { room: Open, ports: [None,    None,    BigArch, Sewer  ], weight: 4 },
        MDef { room: Open, ports: [BigArch, None,    Sewer,   None   ], weight: 3 },
        MDef { room: Open, ports: [BigArch, None,    None,    Sewer  ], weight: 3 },
        MDef { room: Open, ports: [None,    BigArch, Sewer,   None   ], weight: 3 },
        MDef { room: Open, ports: [None,    BigArch, None,    Sewer  ], weight: 3 },
        // ── Sewer tunnels — N-S axis, centre channel ──
        MDef { room: SewerTunnel, ports: [Sewer, Sewer, None,       None      ], weight: 7 },
        MDef { room: SewerTunnel, ports: [Sewer, Sewer, Shaft,      None      ], weight: 4 },
        MDef { room: SewerTunnel, ports: [Sewer, Sewer, None,       Shaft     ], weight: 4 },
        MDef { room: SewerTunnel, ports: [Sewer, Sewer, Shaft,      Shaft     ], weight: 2 },
        // ── Sewer tunnels — E-W axis, centre channel ──
        MDef { room: SewerTunnel, ports: [None,       None,      Sewer, Sewer], weight: 7 },
        MDef { room: SewerTunnel, ports: [Shaft,      None,      Sewer, Sewer], weight: 4 },
        MDef { room: SewerTunnel, ports: [None,       Shaft,     Sewer, Sewer], weight: 4 },
        MDef { room: SewerTunnel, ports: [Shaft,      Shaft,     Sewer, Sewer], weight: 2 },
        // ── Sewer tunnels — N-S axis, double side channels ──
        MDef { room: SewerDouble, ports: [Sewer, Sewer, None,  None ], weight: 4 },
        MDef { room: SewerDouble, ports: [Sewer, Sewer, Shaft, None ], weight: 3 },
        MDef { room: SewerDouble, ports: [Sewer, Sewer, None,  Shaft], weight: 3 },
        // ── Sewer tunnels — E-W axis, double side channels ──
        MDef { room: SewerDouble, ports: [None,  None,  Sewer, Sewer], weight: 4 },
        MDef { room: SewerDouble, ports: [Shaft, None,  Sewer, Sewer], weight: 3 },
        MDef { room: SewerDouble, ports: [None,  Shaft, Sewer, Sewer], weight: 3 },
        // ── Sewer cross-junctions (T and 4-way) ──
        MDef { room: SewerCross, ports: [Sewer, Sewer, Sewer, Sewer], weight: 2 },
        MDef { room: SewerCross, ports: [Sewer, Sewer, Sewer, None ], weight: 3 },
        MDef { room: SewerCross, ports: [Sewer, Sewer, None,  Sewer], weight: 3 },
        MDef { room: SewerCross, ports: [Sewer, None,  Sewer, Sewer], weight: 3 },
        MDef { room: SewerCross, ports: [None,  Sewer, Sewer, Sewer], weight: 3 },
    ]
}

/// BFS labyrinth assembly. Returns sorted cells and extraction cell.
fn gen_grid(
    seed: u64,
    target: usize,
) -> (Vec<((i32,i32), ([ConnType;4], RoomKind))>, (i32,i32)) {
    use std::collections::HashMap;

    let mut rng  = Rng::new(seed);
    let mut grid: HashMap<(i32,i32), ([ConnType;4], RoomKind)> = HashMap::new();
    let mut depth: HashMap<(i32,i32), usize> = HashMap::new();

    // Start: Open room, only BigArch toward +Z (player moves forward into the map)
    grid.insert((0,0), ([ConnType::BigArch, ConnType::None, ConnType::None, ConnType::None], RoomKind::Open));
    depth.insert((0,0), 0);

    let defs = module_defs();
    let mut queue: Vec<(i32,i32,usize)> = vec![(0,0,0)];
    let mut qi = 0;

    while qi < queue.len() {
        let (gx, gz, side) = queue[qi];
        qi += 1;
        let (ngx, ngz) = nb(gx, gz, side);
        if ngx.abs() > 2 || ngz.abs() > 2 { continue; }  // cap to 5×5 grid
        if grid.contains_key(&(ngx, ngz))  { continue; }

        let incoming      = opp_side(side);
        let incoming_conn = grid[&(gx,gz)].0[side];
        let parent_depth  = depth[&(gx,gz)];

        // Filter to modules compatible with incoming_conn and any already-placed neighbours.
        let base_candidates: Vec<&MDef> = defs.iter().filter(|d| {
            if !conn_compat(d.ports[incoming], incoming_conn) { return false; }
            for s in 0..4 {
                if s == incoming { continue; }
                if let Some((np,_)) = grid.get(&nb(ngx, ngz, s)) {
                    if !conn_compat(d.ports[s], np[opp_side(s)]) { return false; }
                }
            }
            true
        }).collect();

        // Once target is reached, prefer dead-ends (single open port).
        let candidates: Vec<&MDef> = if grid.len() >= target {
            let dead: Vec<&MDef> = base_candidates.iter().copied()
                .filter(|d| d.ports.iter().filter(|&&p| p != ConnType::None).count() == 1)
                .collect();
            if dead.is_empty() { base_candidates } else { dead }
        } else {
            base_candidates
        };

        let (chosen_ports, chosen_room) = if candidates.is_empty() {
            // No compatible template: force minimal dead-end
            let mut p = [ConnType::None; 4];
            p[incoming] = incoming_conn;
            (p, RoomKind::Open)
        } else {
            let total: u32 = candidates.iter().map(|d| d.weight).sum();
            let mut roll = (rng.next() as u32) % total;
            let c = candidates.iter()
                .find(|d| { if roll < d.weight { true } else { roll -= d.weight; false } })
                .copied().unwrap_or(candidates[0]);
            (c.ports, c.room)
        };

        grid.insert((ngx,ngz), (chosen_ports, chosen_room));
        depth.insert((ngx,ngz), parent_depth + 1);

        for s in 0..4 {
            if s == incoming || chosen_ports[s] == ConnType::None { continue; }
            let nb_c = nb(ngx, ngz, s);
            if !grid.contains_key(&nb_c) {
                queue.push((ngx, ngz, s));
            }
        }
    }

    // Post-BFS cleanup: clear any port whose neighbour is absent or mismatched.
    // Two-pass (snapshot keys, then mutate) to satisfy borrow checker.
    let cell_positions: Vec<(i32,i32)> = grid.keys().copied().collect();
    let mut mismatches: Vec<((i32,i32), usize)> = Vec::new();
    for &(gx,gz) in &cell_positions {
        let ports = grid[&(gx,gz)].0;
        for s in 0..4usize {
            if ports[s] == ConnType::None { continue; }
            let (nx,nz) = nb(gx,gz,s);
            let bad = match grid.get(&(nx,nz)) {
                None => true,
                Some(&(nports, _)) => !conn_compat(ports[s], nports[opp_side(s)]),
            };
            if bad { mismatches.push(((gx,gz), s)); }
        }
    }
    for ((gx,gz), s) in mismatches {
        grid.get_mut(&(gx,gz)).unwrap().0[s] = ConnType::None;
    }

    // Sewer type propagation: all directly-connected SewerTunnel/SewerDouble cells
    // in a connected component must use the same channel variant, so there is no
    // visual mismatch (single-centre vs double-side streams) at shared openings.
    // SewerCross uses centre-streams internally and forces its component to SewerTunnel.
    {
        let mut visited: std::collections::HashSet<(i32,i32)> = Default::default();
        for &start in &cell_positions {
            if visited.contains(&start) { continue; }
            if !matches!(grid[&start].1, RoomKind::SewerTunnel | RoomKind::SewerDouble | RoomKind::SewerCross) { continue; }
            let mut component: Vec<(i32,i32)> = Vec::new();
            let mut sq: Vec<(i32,i32)> = vec![start];
            let mut sqi = 0;
            visited.insert(start);
            while sqi < sq.len() {
                let pos = sq[sqi]; sqi += 1;
                component.push(pos);
                for s in 0..4usize {
                    if grid[&pos].0[s] != ConnType::Sewer { continue; }
                    let nb_pos = nb(pos.0, pos.1, s);
                    if visited.contains(&nb_pos) { continue; }
                    if let Some(&(_, rk)) = grid.get(&nb_pos) {
                        if matches!(rk, RoomKind::SewerTunnel | RoomKind::SewerDouble | RoomKind::SewerCross) {
                            visited.insert(nb_pos);
                            sq.push(nb_pos);
                        }
                    }
                }
            }
            let has_cross = component.iter().any(|&c| grid[&c].1 == RoomKind::SewerCross);
            let n_double  = component.iter().filter(|&&c| grid[&c].1 == RoomKind::SewerDouble).count();
            let use_double = !has_cross && n_double * 2 > component.len();
            for pos in component {
                if grid[&pos].1 != RoomKind::SewerCross {
                    grid.get_mut(&pos).unwrap().1 = if use_double { RoomKind::SewerDouble } else { RoomKind::SewerTunnel };
                }
            }
        }
    }

    // Extraction: prefer an Open dead-end at depth ≥ 4 (build_open handles the hub room).
    // Fall back to any dead-end, then any deepest cell.
    let is_dead_end = |cell: &(i32,i32)| grid[cell].0.iter().filter(|&&c| c != ConnType::None).count() == 1;
    let extraction = depth.iter()
        .filter(|(cell,d)| **d >= 4 && **cell != (0,0) && is_dead_end(cell) && grid[*cell].1 == RoomKind::Open)
        .max_by_key(|(_,d)| *d).map(|(c,_)| *c)
        .or_else(|| depth.iter()
            .filter(|(cell,d)| **d >= 4 && **cell != (0,0) && is_dead_end(cell))
            .max_by_key(|(_,d)| *d).map(|(c,_)| *c))
        .or_else(|| depth.iter().filter(|(c,_)| **c != (0,0))
            .max_by_key(|(_,d)| *d).map(|(c,_)| *c))
        .unwrap_or((0,3));

    let mut cells: Vec<((i32,i32), ([ConnType;4], RoomKind))> = grid.into_iter().collect();
    cells.sort_by_key(|&((gx,gz),_)| (gz,gx));
    (cells, extraction)
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

fn sa_v(statics: &mut Vec<StaticDef>, kind: StaticKind, pos: Vec3, size: Vec3) {
    statics.push(StaticDef::axis_aligned(kind, pos, size));
}

/// A square-section steel grate bar (rendered with the reinforced-steel
/// `SewerArch` material) spanning `a`→`b`. Used for culvert grates so they read
/// as dark structural steel, not bright pipe metal.
fn grate_bar(st: &mut Vec<StaticDef>, a: Vec3, b: Vec3, d: f32) {
    let delta = b - a;
    let len = delta.length();
    if len < 1e-3 {
        return;
    }
    st.push(StaticDef {
        kind: StaticKind::SewerArch,
        position: (a + b) * 0.5,
        rotation: Quat::from_rotation_arc(Vec3::Z, delta / len),
        size: Vec3::new(d, d, len),
    });
}


/// A ceiling pipe running between `a` and `b` (both at the pipe height, `a` the
/// −axis end, `b` the +axis end). Where an end is flagged it curves smoothly up
/// to meet the ceiling underside and tucks in (no flat disc left hanging in
/// mid-air); otherwise it runs straight to the cell edge (into a wall, or
/// continuing into the next cell). `ceiling_bottom` is the roof underside.
fn ceiling_pipe(
    st: &mut Vec<StaticDef>,
    a: Vec3,
    b: Vec3,
    ceiling_bottom: f32,
    d: f32,
    bend_a: bool,
    bend_b: bool,
) {
    let axis = b - a;
    let len = axis.length();
    if len < 1e-3 {
        return;
    }
    let dir = axis / len;
    // Rise needed to reach the ceiling; the quarter-elbow advances by this much
    // along the run too, so shorten the straight section by the same amount.
    let arc_r = (ceiling_bottom - a.y).max(0.12);
    let a2 = if bend_a { a + dir * arc_r } else { a };
    let b2 = if bend_b { b - dir * arc_r } else { b };

    // Straight section (axis-aligned cylinder).
    let mid = (a2 + b2) * 0.5;
    let seg = (b2 - a2).length();
    if seg > 1e-3 {
        let mut size = Vec3::splat(d);
        if dir.x.abs() > 0.5 {
            size.x = seg;
        } else {
            size.z = seg;
        }
        sa_v(st, StaticKind::SewerPipe, mid, size);
    }

    // Bends up into the roof, as a single smooth swept-tube mesh (no segment
    // gaps). `rotation` maps local +Z→the horizontal out-dir and +Y→up; the
    // client sweeps a circular profile along the quarter-arc + vertical stub.
    let mut into_roof = |start: Vec3, out: Vec3| {
        let (from, to) = (out, Vec3::Y);
        let rot = Quat::from_mat3(&Mat3::from_cols(to.cross(from), to, from));
        let stub_len = (ceiling_bottom + 0.12 - (start.y + arc_r)).max(0.05);
        st.push(StaticDef {
            kind: StaticKind::PipeElbow,
            position: start,
            rotation: rot,
            size: Vec3::new(d * 0.5, arc_r, stub_len),
        });
    };
    if bend_a {
        into_roof(a2, -dir);
    }
    if bend_b {
        into_roof(b2, dir);
    }
}

/// A grated culvert mouth set low in a dead-end wall, at the water channel — the
/// "stream under a road" pipe the sewer water flows into/out of. Built to read
/// as an OPEN hole: a short metal ring recessed into the wall, a dark bore
/// behind it, and closely-spaced round grate bars across the front. `side`
/// selects the wall (0:+Z 1:-Z 2:+X 3:-X); `lats` are the lateral offsets of
/// each stream along that wall; `r` is the mouth radius.
fn culvert(
    st: &mut Vec<StaticDef>,
    li: &mut Vec<LightDef>,
    side: usize,
    cx: f32,
    cz: f32,
    lats: &[f32],
    stream_w: f32,
) {
    use StaticKind::{PipeBore, PipeRing};
    const RING_LEN: f32 = 0.30; // how far the open pipe protrudes from the wall
    // The pipe IS the stream's pipe: opening (inner) radius = half the stream
    // width. The tube is open-ended so you see into it.
    let r = stream_w * 0.5;
    // Drop the mouth to floor level so its width at the floor matches the stream
    // (centre just above the floor → the lower half is buried, the opening sits
    // in the water like a real culvert).
    let y0 = r * 0.2;
    let glow = Color::srgb(0.1, 0.7, 0.3);

    let along_z = side <= 1;
    let sgn = if side == 0 || side == 2 { 1.0 } else { -1.0 };
    // Outward direction from the wall into the room.
    let out = if along_z { Vec3::new(0.0, 0.0, -sgn) } else { Vec3::new(-sgn, 0.0, 0.0) };
    let axis = -out; // tube axis points into the wall
    let rot = Quat::from_rotation_arc(Vec3::Z, axis);
    // In-plane "lateral" direction (along the wall) for the grate cross-hatch.
    let lat_dir = if along_z { Vec3::X } else { Vec3::Z };

    for &lat in lats {
        let centre = if along_z {
            Vec3::new(cx + lat, y0, cz + sgn * GHALF)
        } else {
            Vec3::new(cx + sgn * GHALF, y0, cz + lat)
        };
        // Open-ended pipe (no caps) protruding from the wall — you see into it.
        st.push(StaticDef {
            kind: PipeRing,
            position: centre + out * (RING_LEN * 0.5),
            rotation: rot,
            size: Vec3::new(2.0 * r, 2.0 * r, RING_LEN),
        });
        // Dark, murky-water bore disc at the BACK of the tube (against the wall)
        // so looking in shows water, not the wall metal.
        st.push(StaticDef {
            kind: PipeBore,
            position: centre + out * 0.02,
            rotation: rot,
            size: Vec3::new(2.0 * r * 0.98, 2.0 * r * 0.98, 0.02),
        });
        // Diamond grate at the FRONT opening, recessed just inside the rim. Each
        // bar is an exact chord of the opening (radius r): its endpoints land on
        // the pipe wall — never short, never overshooting.
        let grate_c = centre + out * (RING_LEN - 0.02);
        let bar_d = (r * 0.16).clamp(0.025, 0.045);
        // Clip the bars to half a bar-thickness INSIDE the wall so the square
        // bar's corner sits flush with the pipe wall instead of poking through.
        let rc = r - bar_d * 0.5;
        let d1 = (lat_dir + Vec3::Y).normalize();
        let d2 = (lat_dir - Vec3::Y).normalize();
        for (dir, perp) in [(d1, d2), (d2, d1)] {
            for k in -2..=2 {
                let o = k as f32 * (rc * 0.42);
                if o.abs() >= rc {
                    continue;
                }
                let half = (rc * rc - o * o).sqrt(); // exact half-chord length
                let c = grate_c + perp * o;
                grate_bar(st, c - dir * half, c + dir * half, bar_d);
            }
        }
        li.push(LightDef { position: centre + out * 0.25, color: glow, intensity: 130.0, range: 2.4 });
    }
}
fn pt_v(lights: &mut Vec<LightDef>, pos: Vec3, color: Color, intensity: f32, range: f32) {
    lights.push(LightDef { position: pos, color, intensity, range });
}

/// Opening width for a connection type.
fn conn_w(c: ConnType) -> f32 {
    use ConnType::*;
    match c { BigArch => OPEN_W, Door => DOOR_W,
              Shaft | ShaftLeft | ShaftRight => SHAFT_W,
              Sewer => SEWER_W, None => 0.0 }
}
/// Opening height for a connection type.
fn conn_h(c: ConnType) -> f32 {
    use ConnType::*;
    match c { BigArch => CELL_H, Door => DOOR_H,
              Shaft | ShaftLeft | ShaftRight => SHAFT_W,
              Sewer => SEWER_H, None => 0.0 }
}
/// Lateral shift of a connection's opening along the wall (absolute world coords).
/// ShaftLeft = shifted toward –X (for Z-facing walls) or –Z (for X-facing walls).
const OFFSET_SHIFT: f32 = 2.6;
fn conn_lateral(c: ConnType) -> f32 {
    match c {
        ConnType::ShaftLeft  => -OFFSET_SHIFT,
        ConnType::ShaftRight =>  OFFSET_SHIFT,
        _ => 0.0,
    }
}

/// Emit a wall face with the correct opening cut into it.
/// `wall_c` = wall slab centre, `full` = full extents.
/// `along_x` = the wall's long axis runs along X.
/// ShaftLeft/ShaftRight shift the opening laterally; otherwise it is centred.
fn wall_face(
    st: &mut Vec<StaticDef>, wall_c: Vec3, full: Vec3,
    conn: ConnType, along_x: bool, cx: f32, cz: f32,
) {
    use StaticKind::SewerWall;
    let ow = conn_w(conn);
    let oh = conn_h(conn);
    if ow <= 0.0 { sa_v(st, SewerWall, wall_c, full); return; }

    let lat = conn_lateral(conn);
    let wall_bot = wall_c.y - full.y * 0.5;

    if along_x {
        let open_cx  = cx + lat;
        let left_w   = (full.x * 0.5 + lat - ow * 0.5).max(0.0);
        let right_w  = (full.x * 0.5 - lat - ow * 0.5).max(0.0);
        if left_w  > 0.001 { sa_v(st, SewerWall, Vec3::new(cx - full.x*0.5 + left_w*0.5,  wall_c.y, wall_c.z), Vec3::new(left_w,  full.y, full.z)); }
        if right_w > 0.001 { sa_v(st, SewerWall, Vec3::new(cx + full.x*0.5 - right_w*0.5, wall_c.y, wall_c.z), Vec3::new(right_w, full.y, full.z)); }
        if oh < full.y - 0.05 {
            let lh = full.y - oh;
            sa_v(st, SewerWall, Vec3::new(open_cx, wall_bot + oh + lh*0.5, wall_c.z), Vec3::new(ow, lh, full.z));
        }
    } else {
        let open_cz  = cz + lat;
        let left_w   = (full.z * 0.5 + lat - ow * 0.5).max(0.0);
        let right_w  = (full.z * 0.5 - lat - ow * 0.5).max(0.0);
        if left_w  > 0.001 { sa_v(st, SewerWall, Vec3::new(wall_c.x, wall_c.y, cz - full.z*0.5 + left_w*0.5),  Vec3::new(full.x, full.y, left_w)); }
        if right_w > 0.001 { sa_v(st, SewerWall, Vec3::new(wall_c.x, wall_c.y, cz + full.z*0.5 - right_w*0.5), Vec3::new(full.x, full.y, right_w)); }
        if oh < full.y - 0.05 {
            let lh = full.y - oh;
            sa_v(st, SewerWall, Vec3::new(wall_c.x, wall_bot + oh + lh*0.5, open_cz), Vec3::new(full.x, lh, ow));
        }
    }
}

/// Solid shoulder fill block beside a sewer tunnel, optionally with a shaft corridor cut through it.
/// `axis_along_z` = tunnel runs N-S, so fill is on E or W (shaft corridor runs E-W through fill).
fn shoulder_fill(
    st: &mut Vec<StaticDef>,
    fill_cx: f32, fill_cz: f32,
    fill_w: f32,     // dimension perpendicular to tunnel axis
    fill_len: f32,   // dimension parallel to tunnel axis (= GRID)
    has_shaft: bool,
    axis_along_z: bool, // true = tunnel along Z → fill on E/W → shaft runs along X
) {
    use StaticKind::SewerWall;
    if !has_shaft {
        let (sx, sz) = if axis_along_z { (fill_w, fill_len) } else { (fill_len, fill_w) };
        sa_v(st, SewerWall, Vec3::new(fill_cx, CELL_H*0.5, fill_cz), Vec3::new(sx, CELL_H, sz));
        return;
    }
    // Split into 3 pieces around the SHAFT_W × SHAFT_W shaft opening (centred on the cell).
    let flank = (fill_len - SHAFT_W) * 0.5;
    let fo    = SHAFT_W * 0.5 + flank * 0.5;
    let ah    = CELL_H - SHAFT_W;
    if axis_along_z {
        // Shaft corridor runs along X; flanks run along Z
        sa_v(st, SewerWall, Vec3::new(fill_cx, CELL_H*0.5, fill_cz + fo), Vec3::new(fill_w, CELL_H, flank));
        sa_v(st, SewerWall, Vec3::new(fill_cx, CELL_H*0.5, fill_cz - fo), Vec3::new(fill_w, CELL_H, flank));
        sa_v(st, SewerWall, Vec3::new(fill_cx, SHAFT_W + ah*0.5, fill_cz), Vec3::new(fill_w, ah, SHAFT_W));
    } else {
        // Shaft corridor runs along Z; flanks run along X
        sa_v(st, SewerWall, Vec3::new(fill_cx + fo, CELL_H*0.5, fill_cz), Vec3::new(flank, CELL_H, fill_w));
        sa_v(st, SewerWall, Vec3::new(fill_cx - fo, CELL_H*0.5, fill_cz), Vec3::new(flank, CELL_H, fill_w));
        sa_v(st, SewerWall, Vec3::new(fill_cx, SHAFT_W + ah*0.5, fill_cz), Vec3::new(SHAFT_W, ah, fill_w));
    }
}

// ---------------------------------------------------------------------------
// Module dispatcher
// ---------------------------------------------------------------------------

fn build_module(
    statics: &mut Vec<StaticDef>,
    lights:  &mut Vec<LightDef>,
    props:   &mut Vec<PropDef>,
    items:   &mut Vec<Vec3>,
    enemies: &mut Vec<Vec3>,
    gx: i32, gz: i32,
    raw_ports: [ConnType; 4],
    room: RoomKind,
    placed: &std::collections::HashMap<(i32,i32), ([ConnType;4], RoomKind)>,
    is_start: bool,
    is_extraction: bool,
    rng: &mut Rng,
    depth: u32,
) {
    let cx = gx as f32 * GRID;
    let cz = gz as f32 * GRID;

    // Clamp: close any side whose neighbour wasn't placed
    let mut ports = raw_ports;
    for s in 0..4usize {
        if ports[s] != ConnType::None && !placed.contains_key(&nb(gx, gz, s)) {
            ports[s] = ConnType::None;
        }
    }

    // Neighbour room kind per side — lets ceiling pipes know whether they
    // continue into the next cell or terminate (and need an elbow into the roof).
    let mut nbrs: [Option<RoomKind>; 4] = [None; 4];
    for s in 0..4usize {
        nbrs[s] = placed.get(&nb(gx, gz, s)).map(|(_, rk)| *rk);
    }

    match room {
        RoomKind::Open        => build_open(statics, lights, props, items, enemies, cx, cz, ports, nbrs, is_start, is_extraction, rng, depth),
        RoomKind::SewerTunnel => build_sewer(statics, lights, props, items, enemies, cx, cz, ports, nbrs, false, rng, depth),
        RoomKind::SewerDouble => build_sewer(statics, lights, props, items, enemies, cx, cz, ports, nbrs, true,  rng, depth),
        RoomKind::SewerCross  => build_sewer_cross(statics, lights, props, items, enemies, cx, cz, ports, nbrs, rng, depth),
    }
}

// ---------------------------------------------------------------------------
// Open-room builder
// ---------------------------------------------------------------------------

fn build_open(
    statics: &mut Vec<StaticDef>, lights: &mut Vec<LightDef>,
    props: &mut Vec<PropDef>, items: &mut Vec<Vec3>, enemies: &mut Vec<Vec3>,
    cx: f32, cz: f32, ports: [ConnType; 4], nbrs: [Option<RoomKind>; 4],
    is_start: bool, is_extraction: bool, rng: &mut Rng, depth: u32,
) {
    use StaticKind::*;
    let has_z = ports[0] != ConnType::None || ports[1] != ConnType::None;
    let has_x = ports[2] != ConnType::None || ports[3] != ConnType::None;

    // Extraction room: same footprint as a normal cell but the floor has a square hole,
    // and the hub room is directly below — the cell walls simply extend downward.
    // No separate shaft or ceiling: the extraction floor IS the hub ceiling.
    if is_extraction {
        const HOLE:      f32 = 3.0;   // drop hole side length
        const HUB_DEPTH: f32 = 4.0;   // hub room depth below extraction floor (y=0)
        let hh          = HOLE / 2.0;
        let hub_floor_y = -HUB_DEPTH;              // -4.0
        let full_h      = CELL_H + HUB_DEPTH;      //  8.2  (extraction ceiling to hub floor)
        let full_cy     = (hub_floor_y + CELL_H) * 0.5; //  0.1
        let wt          = WALL_T;

        // ── Ceiling ────────────────────────────────────────────────────────
        sa_v(statics, SewerWall, Vec3::new(cx, CELL_H+wt*0.5, cz), Vec3::new(GRID, wt, GRID));

        // ── Outer walls — full height from hub floor to cell ceiling ───────
        // Solid walls are one piece (no z-fighting junction).
        // Walls with openings use wall_face for the upper half + a solid lower extension.
        let lower_h  = HUB_DEPTH - 0.02;           // leave 2 cm gap to avoid z-fighting
        let lower_cy = hub_floor_y + lower_h * 0.5;
        for s in 0..4usize {
            let (wc_x, wc_z, along_x) = match s {
                0 => (cx,       cz+GHALF, true),
                1 => (cx,       cz-GHALF, true),
                2 => (cx+GHALF, cz,       false),
                _ => (cx-GHALF, cz,       false),
            };
            let full_size  = if along_x { Vec3::new(GRID, full_h, wt) } else { Vec3::new(wt, full_h, GRID) };
            let lower_size = if along_x { Vec3::new(GRID, lower_h, wt) } else { Vec3::new(wt, lower_h, GRID) };
            if ports[s] == ConnType::None {
                sa_v(statics, SewerWall, Vec3::new(wc_x, full_cy, wc_z), full_size);
            } else {
                wall_face(statics, Vec3::new(wc_x, CELL_H*0.5, wc_z),
                          if along_x { Vec3::new(GRID, CELL_H, wt) } else { Vec3::new(wt, CELL_H, GRID) },
                          ports[s], along_x, cx, cz);
                sa_v(statics, SewerWall, Vec3::new(wc_x, lower_cy, wc_z), lower_size);
            }
        }

        // ── Extraction floor: 4 planks with square hole ────────────────────
        // The two side strips (left/right) span full GRID in Z.
        // The cap planks (front/back) are 1 cm narrower in X to avoid coplanar
        // interior side-faces with the outer strips (which causes z-fighting).
        let fseg  = (GRID - HOLE) * 0.5;
        let foff  = hh + fseg * 0.5;
        let cap_w = HOLE - 0.01; // break coplanarity
        for (dx, dz, sx, sz) in [(foff,0.0,fseg,GRID),(-foff,0.0,fseg,GRID),(0.0,foff,cap_w,fseg),(0.0,-foff,cap_w,fseg)] {
            sa_v(statics, SewerFloor, Vec3::new(cx+dx, -0.1, cz+dz), Vec3::new(sx, 0.2, sz));
        }
        // Glowing neon ring raised 5 cm above floor surface (no z-fighting)
        let nr = hh + 0.04;
        for (dx, dz, sx, sz) in [(nr,0.0,0.08,HOLE),(-nr,0.0,0.08,HOLE),(0.0,nr,HOLE,0.08),(0.0,-nr,HOLE,0.08)] {
            sa_v(statics, Neon, Vec3::new(cx+dx, 0.05, cz+dz), Vec3::new(sx, 0.05, sz));
        }

        // ── Hub floor (SewerFloor → has physics collider) ──────────────────
        sa_v(statics, SewerFloor, Vec3::new(cx, hub_floor_y-0.1, cz), Vec3::new(GRID, 0.2, GRID));

        // ── Hub contents ───────────────────────────────────────────────────
        let strip_h = hub_floor_y + HUB_DEPTH * 0.62;
        let wall_in = GHALF - wt - 0.04;
        // Shop terminal
        sa_v(statics, Platform, Vec3::new(cx, hub_floor_y+0.8, cz-4.0), Vec3::new(2.5, 1.0, 1.0));
        sa_v(statics, Neon, Vec3::new(cx, hub_floor_y+1.32, cz-4.0), Vec3::new(2.4, 0.06, 0.06));
        // Wall neons
        for s in [-1.0_f32, 1.0] {
            sa_v(statics, Neon, Vec3::new(cx+s*wall_in, strip_h, cz), Vec3::new(0.06, 0.06, GRID*0.8));
            sa_v(statics, Neon, Vec3::new(cx, strip_h, cz+s*wall_in), Vec3::new(GRID*0.8, 0.06, 0.06));
        }
        // Lights
        pt_v(lights, Vec3::new(cx, hub_floor_y+HUB_DEPTH*0.75, cz), Color::srgb(0.3, 0.65, 1.0), 2500.0, 10.0);
        pt_v(lights, Vec3::new(cx, hub_floor_y+HUB_DEPTH*0.75, cz+3.5), Color::srgb(0.5, 0.82, 0.4), 700.0, 5.5);
        pt_v(lights, Vec3::new(cx, hub_floor_y+HUB_DEPTH*0.75, cz-4.0), Color::srgb(0.6, 0.9, 0.5), 700.0, 5.5);
        // Green up-light in the hole to draw the eye
        pt_v(lights, Vec3::new(cx, -0.8, cz), Color::srgb(0.08, 1.0, 0.35), 1400.0, 5.5);

        items.push(Vec3::new(cx, hub_floor_y+0.6, cz+3.0));
        return;
    }

    // Full floor
    sa_v(statics, SewerFloor, Vec3::new(cx, -0.1, cz), Vec3::new(GRID, 0.2, GRID));
    // Ceiling (skip for start — open-top drop shaft)
    if !is_start {
        sa_v(statics, SewerWall, Vec3::new(cx, CELL_H+WALL_T*0.5, cz), Vec3::new(GRID, WALL_T, GRID));
    }
    // Outer walls with typed openings
    wall_face(statics, Vec3::new(cx, CELL_H*0.5, cz+GHALF), Vec3::new(GRID, CELL_H, WALL_T), ports[0], true, cx, cz);
    wall_face(statics, Vec3::new(cx, CELL_H*0.5, cz-GHALF), Vec3::new(GRID, CELL_H, WALL_T), ports[1], true, cx, cz);
    wall_face(statics, Vec3::new(cx+GHALF, CELL_H*0.5, cz), Vec3::new(WALL_T, CELL_H, GRID), ports[2], false, cx, cz);
    wall_face(statics, Vec3::new(cx-GHALF, CELL_H*0.5, cz), Vec3::new(WALL_T, CELL_H, GRID), ports[3], false, cx, cz);

    // Start: drop-shaft neons only, no content
    if is_start {
        let neon_y    = CELL_H * 0.62;
        let wall_in   = GHALF - WALL_T * 0.5 - 0.04;
        for s in [-1.0_f32, 1.0] {
            sa_v(statics, Neon, Vec3::new(cx + s*wall_in, neon_y, cz), Vec3::new(0.06, 0.06, GRID*0.75));
        }
        pt_v(lights, Vec3::new(cx, CELL_H-0.5, cz), Color::srgb(0.6, 0.8, 1.0), 3000.0, 7.5);
        return;
    }

    // Ceiling pipes — hang just below the roof (visible/attached, not buried in
    // the slab) and bend up into it where they'd otherwise end in mid-air.
    let pipe_y = CELL_H - 0.30;
    let ceil_b = CELL_H;
    let open_bend = |s: usize| ports[s] != ConnType::None && nbrs[s] != Some(RoomKind::Open);
    if has_z {
        ceiling_pipe(statics, Vec3::new(cx-0.9, pipe_y, cz-GHALF), Vec3::new(cx-0.9, pipe_y, cz+GHALF), ceil_b, 0.26, open_bend(1), open_bend(0));
    }
    if has_x {
        ceiling_pipe(statics, Vec3::new(cx-GHALF, pipe_y, cz-0.9), Vec3::new(cx+GHALF, pipe_y, cz-0.9), ceil_b, 0.26, open_bend(3), open_bend(2));
    }

    // Wall neon strips — only on SOLID walls. Putting a strip on a wall that has
    // a doorway makes the light stretch across the opening (unrealistic, and you'd
    // clip it walking through).
    let neon_y  = CELL_H * 0.72;
    let wall_in = GHALF - WALL_T * 0.5 - 0.04;
    if has_z {
        if ports[2] == ConnType::None { sa_v(statics, Neon, Vec3::new(cx+wall_in, neon_y, cz), Vec3::new(0.06, 0.06, GRID*0.88)); }
        if ports[3] == ConnType::None { sa_v(statics, Neon, Vec3::new(cx-wall_in, neon_y, cz), Vec3::new(0.06, 0.06, GRID*0.88)); }
    }
    if has_x {
        if ports[0] == ConnType::None { sa_v(statics, Neon, Vec3::new(cx, neon_y, cz+wall_in), Vec3::new(GRID*0.88, 0.06, 0.06)); }
        if ports[1] == ConnType::None { sa_v(statics, Neon, Vec3::new(cx, neon_y, cz-wall_in), Vec3::new(GRID*0.88, 0.06, 0.06)); }
    }

    let open_count = ports.iter().filter(|&&c| c != ConnType::None).count();
    match open_count {
        1 => {
            items.push(Vec3::new(cx, 0.6, cz));
            pt_v(lights, Vec3::new(cx, 2.0, cz), Color::srgb(0.85, 0.62, 0.12), 400.0, 4.5);
        }
        2 => {
            pt_v(lights, Vec3::new(cx, CELL_H-0.3, cz), Color::srgb(0.2, 0.9, 1.0), 900.0, 6.0);
            if rng.coin(0.5) {
                let s  = if rng.coin(0.5) { 1.0_f32 } else { -1.0 };
                let (ox, oz) = if rng.coin(0.5) { (GHALF-0.4, rng.range(-2.0, 2.0)) } else { (rng.range(-2.0, 2.0), GHALF-0.4) };
                pt_v(lights, Vec3::new(cx+ox*s, CELL_H*0.55, cz+oz), Color::srgb(1.0, 0.42, 0.05), 550.0, 3.5);
            }
            if rng.coin(0.55) {
                let closed: Vec<usize> = (0..4).filter(|&s| ports[s] == ConnType::None).collect();
                if !closed.is_empty() {
                    let cs = closed[rng.umod(closed.len())];
                    let (px, pz) = corner_pos(cs, cx, cz, rng);
                    let mut cy = 0.42_f32;
                    for i in 0..(1 + rng.umod(3)) {
                        props.push(PropDef { shape: crate::props::PropShape::Crate { size: Vec3::splat(0.72 - i as f32 * 0.04) }, position: Vec3::new(px + rng.range(-0.08, 0.08), cy, pz), density: 85.0 });
                        cy += 0.74;
                    }
                }
            }
            let is_corner = (ports[0] != ConnType::None || ports[1] != ConnType::None) && (ports[2] != ConnType::None || ports[3] != ConnType::None);
            if is_corner && depth > 0 && rng.coin(0.45) {
                enemies.push(Vec3::new(cx + rng.range(-1.0, 1.0), 1.0, cz + rng.range(-1.0, 1.0)));
            }
        }
        _ => {
            pt_v(lights, Vec3::new(cx, CELL_H+0.2, cz), Color::srgb(0.75, 0.82, 1.0), 3500.0, 9.0);
            let n_e = 1 + (depth as usize / 2).min(2) + rng.umod(2);
            for _ in 0..n_e {
                enemies.push(Vec3::new(cx + rng.range(-2.0, 2.0), 1.0, cz + rng.range(-2.0, 2.0)));
            }
        }
    }
}

fn corner_pos(side: usize, cx: f32, cz: f32, rng: &mut Rng) -> (f32, f32) {
    match side {
        0 => (cx + rng.range(-1.2, 1.2), cz + GHALF - 1.8),
        1 => (cx + rng.range(-1.2, 1.2), cz - GHALF + 1.8),
        2 => (cx + GHALF - 1.8, cz + rng.range(-1.2, 1.2)),
        _ => (cx - GHALF + 1.8, cz + rng.range(-1.2, 1.2)),
    }
}

// ---------------------------------------------------------------------------
// Sewer-tunnel builder
// ---------------------------------------------------------------------------

/// Narrow arched sewer channel. `double_channel`: two side streams + centre walkway.
fn build_sewer(
    statics: &mut Vec<StaticDef>, lights: &mut Vec<LightDef>,
    _props: &mut Vec<PropDef>, items: &mut Vec<Vec3>, enemies: &mut Vec<Vec3>,
    cx: f32, cz: f32, ports: [ConnType; 4], nbrs: [Option<RoomKind>; 4],
    double_channel: bool, rng: &mut Rng, depth: u32,
) {
    use StaticKind::*;

    let along_z = ports[0] == ConnType::Sewer || ports[1] == ConnType::Sewer;

    // Floor + low ceiling
    sa_v(statics, SewerFloor, Vec3::new(cx, -0.1,             cz), Vec3::new(GRID, 0.2,    GRID));
    sa_v(statics, SewerWall,  Vec3::new(cx, SEWER_H+WALL_T*0.5, cz), Vec3::new(GRID, WALL_T, GRID));

    let fill_e_cx = cx + SEWER_W*0.5 + SEWER_FILL*0.5;
    let fill_w_cx = cx - SEWER_W*0.5 - SEWER_FILL*0.5;
    let fill_n_cz = cz + SEWER_W*0.5 + SEWER_FILL*0.5;
    let fill_s_cz = cz - SEWER_W*0.5 - SEWER_FILL*0.5;

    // Helper: has shaft port on a given side (any shaft variant)
    let is_shaft = |p: ConnType| matches!(p, ConnType::Shaft | ConnType::ShaftLeft | ConnType::ShaftRight);

    // Outer walls + shoulder fills
    if along_z {
        wall_face(statics, Vec3::new(cx, CELL_H*0.5, cz+GHALF), Vec3::new(GRID, CELL_H, WALL_T), ports[0], true, cx, cz);
        wall_face(statics, Vec3::new(cx, CELL_H*0.5, cz-GHALF), Vec3::new(GRID, CELL_H, WALL_T), ports[1], true, cx, cz);
        wall_face(statics, Vec3::new(cx+GHALF, CELL_H*0.5, cz), Vec3::new(WALL_T, CELL_H, GRID), ports[2], false, cx, cz);
        wall_face(statics, Vec3::new(cx-GHALF, CELL_H*0.5, cz), Vec3::new(WALL_T, CELL_H, GRID), ports[3], false, cx, cz);
        shoulder_fill(statics, fill_e_cx, cz, SEWER_FILL, GRID, is_shaft(ports[2]), true);
        shoulder_fill(statics, fill_w_cx, cz, SEWER_FILL, GRID, is_shaft(ports[3]), true);
    } else {
        wall_face(statics, Vec3::new(cx, CELL_H*0.5, cz+GHALF), Vec3::new(GRID, CELL_H, WALL_T), ports[0], true, cx, cz);
        wall_face(statics, Vec3::new(cx, CELL_H*0.5, cz-GHALF), Vec3::new(GRID, CELL_H, WALL_T), ports[1], true, cx, cz);
        wall_face(statics, Vec3::new(cx+GHALF, CELL_H*0.5, cz), Vec3::new(WALL_T, CELL_H, GRID), ports[2], false, cx, cz);
        wall_face(statics, Vec3::new(cx-GHALF, CELL_H*0.5, cz), Vec3::new(WALL_T, CELL_H, GRID), ports[3], false, cx, cz);
        shoulder_fill(statics, cx, fill_n_cz, SEWER_FILL, GRID, is_shaft(ports[0]), false);
        shoulder_fill(statics, cx, fill_s_cz, SEWER_FILL, GRID, is_shaft(ports[1]), false);
    }

    // ── Water channels ──────────────────────────────────────────────────────
    // Water plane sits just above floor (y=0.01) so it is visible above the floor slab.
    let cw = 0.55_f32;  // channel width

    if double_channel {
        // Two side streams, centre walkway
        let chan_off = SEWER_W * 0.5 - cw * 0.5;
        for s in [-1.0_f32, 1.0] {
            if along_z {
                sa_v(statics, SewerWater, Vec3::new(cx + s*chan_off, 0.01, cz), Vec3::new(cw, 0.04, GRID));
            } else {
                sa_v(statics, SewerWater, Vec3::new(cx, 0.01, cz + s*chan_off), Vec3::new(GRID, 0.04, cw));
            }
        }
        // Centre walkway raised lip (subtle kerb)
        let ww = SEWER_W - cw * 2.0;
        if along_z { sa_v(statics, SewerFloor, Vec3::new(cx, 0.03, cz), Vec3::new(ww, 0.06, GRID)); }
        else        { sa_v(statics, SewerFloor, Vec3::new(cx, 0.03, cz), Vec3::new(GRID, 0.06, ww)); }
    } else {
        // Single centre stream, side walkways
        if along_z {
            sa_v(statics, SewerWater, Vec3::new(cx, 0.01, cz), Vec3::new(cw, 0.04, GRID));
            let ww = (SEWER_W - cw) * 0.5;
            for s in [-1.0_f32, 1.0] {
                sa_v(statics, SewerFloor, Vec3::new(cx + s*(cw*0.5+ww*0.5), 0.03, cz), Vec3::new(ww, 0.06, GRID));
            }
        } else {
            sa_v(statics, SewerWater, Vec3::new(cx, 0.01, cz), Vec3::new(GRID, 0.04, cw));
            let ww = (SEWER_W - cw) * 0.5;
            for s in [-1.0_f32, 1.0] {
                sa_v(statics, SewerFloor, Vec3::new(cx, 0.03, cz + s*(cw*0.5+ww*0.5)), Vec3::new(GRID, 0.06, ww));
            }
        }
    }

    // ── Ceiling pipe ──────────────────────────────────────────────────────
    // Runs the tunnel axis, hanging just below the roof. Where the run continues
    // into another sewer cell it stays straight; where it would dead-end into
    // mid-air (e.g. a gateway into an open room) it curves up into the roof.
    let pipe_y = SEWER_H - 0.30;
    let pd = 0.24_f32;
    let ceil_bottom = SEWER_H;
    // A tunnel-axis side terminates the pipe when it opens onto a non-sewer room.
    let pipe_ends = |s: usize| {
        ports[s] == ConnType::Sewer
            && !matches!(
                nbrs[s],
                Some(RoomKind::SewerTunnel | RoomKind::SewerDouble | RoomKind::SewerCross)
            )
    };
    if along_z {
        ceiling_pipe(
            statics,
            Vec3::new(cx, pipe_y, cz - GHALF),
            Vec3::new(cx, pipe_y, cz + GHALF),
            ceil_bottom, pd, pipe_ends(1), pipe_ends(0),
        );
    } else {
        ceiling_pipe(
            statics,
            Vec3::new(cx - GHALF, pipe_y, cz),
            Vec3::new(cx + GHALF, pipe_y, cz),
            ceil_bottom, pd, pipe_ends(3), pipe_ends(2),
        );
    }

    // ── Arch ribs (4 per tunnel, evenly spaced) ───────────────────────────
    let rib_t = 0.12_f32;
    // Posts run the FULL height from floor to the top bar — stopping short looked
    // structurally unsound (bars floating, not reaching the roof).
    let rib_h = SEWER_H - rib_t;
    for i in 0..4usize {
        let t = -0.4 + i as f32 * 0.27;   // evenly spread: -0.4, -0.13, +0.13, +0.40
        let (rx, rz) = if along_z { (cx, cz + GRID*t) } else { (cx + GRID*t, cz) };
        // Top horizontal bar
        if along_z { sa_v(statics, SewerArch, Vec3::new(rx, SEWER_H-rib_t*0.5, rz), Vec3::new(SEWER_W, rib_t, rib_t)); }
        else        { sa_v(statics, SewerArch, Vec3::new(rx, SEWER_H-rib_t*0.5, rz), Vec3::new(rib_t, rib_t, SEWER_W)); }
        // Side posts
        for s in [-1.0_f32, 1.0] {
            let post_pos = if along_z { Vec3::new(rx + s*SEWER_W*0.5, rib_h*0.5, rz) }
                           else        { Vec3::new(rx, rib_h*0.5, rz + s*SEWER_W*0.5) };
            sa_v(statics, SewerArch, post_pos, Vec3::new(rib_t, rib_h, rib_t));
        }
    }

    // ── Drip lights ───────────────────────────────────────────────────────
    for i in 0..3usize {
        let t = -0.35 + i as f32 * 0.35;
        let lp = if along_z { Vec3::new(cx, SEWER_H-0.5, cz + GRID*t) }
                 else        { Vec3::new(cx + GRID*t, SEWER_H-0.5, cz) };
        pt_v(lights, lp, Color::srgb(0.05, 0.65, 0.85), 320.0, 4.5);
    }

    // ── Neon edge strips ──────────────────────────────────────────────────
    // Kept short of the cell edges so they don't reach across the tunnel
    // openings at either end.
    for s in [-1.0_f32, 1.0] {
        if along_z { sa_v(statics, Neon, Vec3::new(cx + s*(SEWER_W*0.5+0.04), SEWER_H-0.08, cz), Vec3::new(0.06, 0.06, GRID*0.6)); }
        else        { sa_v(statics, Neon, Vec3::new(cx, SEWER_H-0.08, cz + s*(SEWER_W*0.5+0.04)), Vec3::new(GRID*0.6, 0.06, 0.06)); }
    }

    // ── Culvert mouths at dead-end sides ───────────────────────────────────
    // Where a tunnel axis dead-ends, the water channel meets the wall through a
    // grated round culvert mouth (a "stream under a road" pipe), one per stream.
    let chan_off = SEWER_W * 0.5 - cw * 0.5;
    let lats: Vec<f32> = if double_channel { vec![-chan_off, chan_off] } else { vec![0.0] };
    if along_z {
        if ports[0] == ConnType::None { culvert(statics, lights, 0, cx, cz, &lats, cw); }
        if ports[1] == ConnType::None { culvert(statics, lights, 1, cx, cz, &lats, cw); }
    } else {
        if ports[2] == ConnType::None { culvert(statics, lights, 2, cx, cz, &lats, cw); }
        if ports[3] == ConnType::None { culvert(statics, lights, 3, cx, cz, &lats, cw); }
    }

    if ports.iter().filter(|&&c| c != ConnType::None).count() == 1 {
        items.push(Vec3::new(cx, 0.6, cz));
    }
    if depth > 0 && rng.coin(0.35) {
        enemies.push(Vec3::new(cx + rng.range(-0.7, 0.7), 1.0, cz + rng.range(-0.7, 0.7)));
    }
}

// ---------------------------------------------------------------------------
// Sewer cross-junction builder
// ---------------------------------------------------------------------------

/// T or 4-way sewer junction: channels run along both axes, four corner walkways.
fn build_sewer_cross(
    statics: &mut Vec<StaticDef>, lights: &mut Vec<LightDef>,
    _props: &mut Vec<PropDef>, items: &mut Vec<Vec3>, enemies: &mut Vec<Vec3>,
    cx: f32, cz: f32, ports: [ConnType; 4], nbrs: [Option<RoomKind>; 4], rng: &mut Rng, depth: u32,
) {
    use StaticKind::*;

    sa_v(statics, SewerFloor, Vec3::new(cx, -0.1,              cz), Vec3::new(GRID, 0.2,    GRID));
    sa_v(statics, SewerWall,  Vec3::new(cx, SEWER_H+WALL_T*0.5, cz), Vec3::new(GRID, WALL_T, GRID));

    // Outer walls
    for (side, along_x) in [(0,true),(1,true),(2,false),(3,false)] {
        let wc = match side {
            0 => Vec3::new(cx,        CELL_H*0.5, cz+GHALF),
            1 => Vec3::new(cx,        CELL_H*0.5, cz-GHALF),
            2 => Vec3::new(cx+GHALF,  CELL_H*0.5, cz),
            _ => Vec3::new(cx-GHALF,  CELL_H*0.5, cz),
        };
        let full = if along_x { Vec3::new(GRID, CELL_H, WALL_T) } else { Vec3::new(WALL_T, CELL_H, GRID) };
        wall_face(statics, wc, full, ports[side], along_x, cx, cz);
    }

    // Shoulder fill in corners where no sewer port
    let cw = 0.55_f32;
    let corner_fill = SEWER_FILL;
    // Each corner block fills the quadrant between two shoulder fills
    let has_z = ports[0] != ConnType::None || ports[1] != ConnType::None;
    let has_x = ports[2] != ConnType::None || ports[3] != ConnType::None;

    if !has_z {
        // E-W only: fill N and S shoulders
        shoulder_fill(statics, cx,  cz + SEWER_W*0.5 + corner_fill*0.5, corner_fill, GRID, false, false);
        shoulder_fill(statics, cx,  cz - SEWER_W*0.5 - corner_fill*0.5, corner_fill, GRID, false, false);
    } else if !has_x {
        // N-S only: fill E and W shoulders
        shoulder_fill(statics, cx + SEWER_W*0.5 + corner_fill*0.5, cz, corner_fill, GRID, false, true);
        shoulder_fill(statics, cx - SEWER_W*0.5 - corner_fill*0.5, cz, corner_fill, GRID, false, true);
    } else {
        // Junction: corner blocks at four quadrant corners
        for (sx, sz) in [(-1.0_f32,-1.0_f32),(-1.0,1.0),(1.0,-1.0),(1.0,1.0)] {
            let bx = cx + sx * (SEWER_W*0.5 + corner_fill*0.5);
            let bz = cz + sz * (SEWER_W*0.5 + corner_fill*0.5);
            sa_v(statics, SewerWall, Vec3::new(bx, CELL_H*0.5, bz), Vec3::new(corner_fill, CELL_H, corner_fill));
        }
    }

    // Cross water channels. The N-S plane runs full length; the E-W stream is
    // split into two arms that ABUT the N-S channel rather than crossing it, so
    // no two translucent water planes ever overlap (which used to double the
    // opacity into a dense block at the junction).
    sa_v(statics, SewerWater, Vec3::new(cx, 0.01, cz), Vec3::new(cw, 0.04, GRID)); // N-S stream
    let arm_len = GHALF - cw * 0.5;
    let arm_off = cw * 0.5 + arm_len * 0.5;
    for s in [-1.0_f32, 1.0] {
        sa_v(statics, SewerWater, Vec3::new(cx + s * arm_off, 0.01, cz), Vec3::new(arm_len, 0.04, cw));
    }

    // Walkways in the four corners of the intersection
    let ww = (SEWER_W - cw) * 0.5;
    for (sx, sz) in [(-1.0_f32,-1.0_f32),(-1.0,1.0),(1.0,-1.0),(1.0,1.0)] {
        sa_v(statics, SewerFloor, Vec3::new(cx + sx*(cw*0.5+ww*0.5), 0.01, cz + sz*(cw*0.5+ww*0.5)), Vec3::new(ww, 0.02, ww));
    }

    // Ceiling pipe cross — each arm hangs just below the roof, bending up into
    // it where it opens onto a non-sewer room instead of stopping in mid-air.
    let pipe_y = SEWER_H - 0.30;
    let pd = 0.24_f32;
    let ceil_bottom = SEWER_H;
    let pipe_ends = |s: usize| {
        ports[s] == ConnType::Sewer
            && !matches!(
                nbrs[s],
                Some(RoomKind::SewerTunnel | RoomKind::SewerDouble | RoomKind::SewerCross)
            )
    };
    ceiling_pipe(
        statics,
        Vec3::new(cx, pipe_y, cz - GHALF),
        Vec3::new(cx, pipe_y, cz + GHALF),
        ceil_bottom, pd, pipe_ends(1), pipe_ends(0),
    );
    ceiling_pipe(
        statics,
        Vec3::new(cx - GHALF, pipe_y, cz),
        Vec3::new(cx + GHALF, pipe_y, cz),
        ceil_bottom, pd, pipe_ends(3), pipe_ends(2),
    );

    // Corner neon strips and central light
    pt_v(lights, Vec3::new(cx, SEWER_H-0.4, cz), Color::srgb(0.1, 0.8, 0.5), 600.0, 6.0);
    for i in 0..3usize {
        let t = -0.35 + i as f32 * 0.35;
        pt_v(lights, Vec3::new(cx + GRID*t, SEWER_H-0.5, cz), Color::srgb(0.05, 0.65, 0.85), 260.0, 4.0);
        pt_v(lights, Vec3::new(cx, SEWER_H-0.5, cz + GRID*t), Color::srgb(0.05, 0.65, 0.85), 260.0, 4.0);
    }

    // Culvert mouths on any dead-end sides of the junction (single centre stream).
    for side in 0..4usize {
        if ports[side] != ConnType::None { continue; }
        culvert(statics, lights, side, cx, cz, &[0.0], cw);
    }

    if depth > 0 && rng.coin(0.45) {
        enemies.push(Vec3::new(cx + rng.range(-0.5, 0.5), 1.0, cz + rng.range(-0.5, 0.5)));
    }
    let _ = items; // junctions rarely have items
}

// ---------------------------------------------------------------------------
// Stretch assembly
// ---------------------------------------------------------------------------

pub fn gen_sewer_stretch(seed: u64, params: StretchParams) -> LevelDef {
    use std::collections::HashMap;

    let target = 8 + (params.depth as usize * 2).min(4);  // 8–12 cells in a 5×5 grid
    let (cells, extraction_cell) = gen_grid(seed, target);
    let placed: HashMap<(i32,i32), ([ConnType;4], RoomKind)> = cells.iter().cloned().collect();
    let depth_map: HashMap<(i32,i32), usize> = {
        // Rebuild depth from BFS order (cells are sorted by gz,gx; we just re-BFS here lightly)
        let mut d = HashMap::new();
        d.insert((0i32,0i32), 0usize);
        for ((gx,gz),_) in &cells {
            if !d.contains_key(&(*gx,*gz)) { d.insert((*gx,*gz), 99); }
        }
        d
    };

    let mut rng     = Rng::new(seed ^ 0xFEED_CAFE_DEAD_BEEF);
    let mut statics = Vec::new();
    let mut lights  = Vec::new();
    let mut props   = Vec::new();
    let mut items   = Vec::new();
    let mut enemies = Vec::new();

    for ((gx, gz), (ports, room)) in &cells {
        let is_start      = (*gx, *gz) == (0, 0);
        let is_extraction = (*gx, *gz) == extraction_cell;
        build_module(&mut statics, &mut lights, &mut props, &mut items, &mut enemies,
            *gx, *gz, *ports, *room, &placed, is_start, is_extraction, &mut rng, params.depth);
    }

    // Build GridCells for the TAB minimap.
    // Clamp ports the same way build_module does so the minimap matches the geometry.
    let grid_cells: Vec<GridCell> = cells.iter().map(|((gx,gz),(ports,room))| {
        let mut clamped = *ports;
        for s in 0..4usize {
            if clamped[s] != ConnType::None && !placed.contains_key(&nb(*gx,*gz,s)) {
                clamped[s] = ConnType::None;
            }
        }
        GridCell {
            gx: *gx, gz: *gz, room: *room, ports: clamped,
            is_start:      (*gx,*gz) == (0,0),
            is_extraction: (*gx,*gz) == extraction_cell,
        }
    }).collect();
    let _ = depth_map;

    let ex_wx = extraction_cell.0 as f32 * GRID;
    let ex_wz = extraction_cell.1 as f32 * GRID;
    LevelDef {
        id: params.id,
        statics, props, lights,
        item_spawns: items,
        player_spawns: vec![
            Vec3::new(-0.6, 2.5, -0.6),
            Vec3::new( 0.6, 2.5, -0.6),
            Vec3::new(-0.6, 2.5,  0.6),
            Vec3::new( 0.6, 2.5,  0.6),
        ],
        // Hub floor at y = -4.0; flag points to hub interior (~standing height).
        extraction: Some(Vec3::new(ex_wx, -3.0, ex_wz)),
        enemy_spawns: enemies,
        grid_cells,
    }
}

// ---------------------------------------------------------------------------
// Named stretch wrappers (used by stretch_graph in run.rs)
// ---------------------------------------------------------------------------

pub fn sewer_entry_level(seed: u64) -> LevelDef {
    gen_sewer_stretch(seed, StretchParams { id: "sewer_entry".into(), depth: 0 })
}

pub fn sewer_branch_a_level(seed: u64) -> LevelDef {
    gen_sewer_stretch(seed ^ 0xABCD_1234_5678_0000, StretchParams { id: "sewer_branch_a".into(), depth: 1 })
}

pub fn sewer_branch_b_level(seed: u64) -> LevelDef {
    gen_sewer_stretch(seed ^ 0x5678_FEDC_BA98_0000, StretchParams { id: "sewer_branch_b".into(), depth: 2 })
}

// ---------------------------------------------------------------------------
// Hub rooms (static, seed ignored)
// ---------------------------------------------------------------------------

pub fn hub_medbay_level(_seed: u64) -> LevelDef {
    hub_room("hub_medbay", 18.0)
}

pub fn hub_armory_level(_seed: u64) -> LevelDef {
    hub_room("hub_armory", 16.0)
}

pub fn hub_intel_level(_seed: u64) -> LevelDef {
    hub_room("hub_intel", 14.0)
}

/// A square hub room with a shop terminal platform.
fn hub_room(id: &str, size: f32) -> LevelDef {
    use StaticKind::*;
    let half = size / 2.0;
    let h = 4.0;
    let mut statics = vec![
        StaticDef::axis_aligned(SewerWalkway, Vec3::new(0.0, 0.15, 0.0), Vec3::new(size, 0.3, size)),
        StaticDef::axis_aligned(SewerWall, Vec3::new(0.0, h / 2.0, -half), Vec3::new(size, h, 0.45)),
        StaticDef::axis_aligned(SewerWall, Vec3::new(0.0, h / 2.0, half), Vec3::new(size, h, 0.45)),
        StaticDef::axis_aligned(SewerWall, Vec3::new(-half, h / 2.0, 0.0), Vec3::new(0.45, h, size)),
        StaticDef::axis_aligned(SewerWall, Vec3::new(half, h / 2.0, 0.0), Vec3::new(0.45, h, size)),
        // Shop terminal — glowing top edge on the counter, not floating above it.
        StaticDef::axis_aligned(Neon, Vec3::new(0.0, 1.02, -4.0), Vec3::new(2.4, 0.06, 0.06)),
        StaticDef::axis_aligned(Platform, Vec3::new(0.0, 0.5, -4.0), Vec3::new(2.5, 1.0, 1.0)),
    ];
    // Wall neon strips — one per wall, flush to the surface.
    let strip_h = 2.6_f32;
    let inset = 0.06_f32;
    // North / south walls run along X
    for sign_z in [-1.0_f32, 1.0] {
        statics.push(StaticDef::axis_aligned(
            Neon,
            Vec3::new(0.0, strip_h, sign_z * (half - inset)),
            Vec3::new(size * 0.55, 0.06, 0.06),
        ));
    }
    // East / west walls run along Z
    for sign_x in [-1.0_f32, 1.0] {
        statics.push(StaticDef::axis_aligned(
            Neon,
            Vec3::new(sign_x * (half - inset), strip_h, 0.0),
            Vec3::new(0.06, 0.06, size * 0.55),
        ));
    }

    // Hub lights — brighter than stretches (safe zone feel)
    let mut lights = vec![
        LightDef {
            position: Vec3::new(0.0, h + 0.2, 0.0),
            color: Color::srgb(0.9, 0.92, 1.0),
            intensity: 5000.0,
            range: size * 0.7,
        },
    ];
    // Cyan accent lights — one per wall, close to the neon strips
    for sign in [-1.0_f32, 1.0] {
        lights.push(LightDef {
            position: Vec3::new(0.0, 2.8, sign * (half - 0.3)),
            color: Color::srgb(0.2, 0.9, 1.0),
            intensity: 600.0,
            range: 5.0,
        });
        lights.push(LightDef {
            position: Vec3::new(sign * (half - 0.3), 2.8, 0.0),
            color: Color::srgb(0.2, 0.9, 1.0),
            intensity: 600.0,
            range: 5.0,
        });
    }
    // Shop terminal glow
    lights.push(LightDef {
        position: Vec3::new(0.0, 1.5, -4.0),
        color: Color::srgb(0.2, 0.9, 1.0),
        intensity: 1200.0,
        range: 4.0,
    });

    LevelDef {
        id: id.into(),
        statics,
        props: vec![],
        lights,
        item_spawns: vec![],
        player_spawns: vec![
            Vec3::new(-2.0, 1.0, 2.0),
            Vec3::new(2.0, 1.0, 2.0),
            Vec3::new(-2.0, 1.0, 6.0),
            Vec3::new(2.0, 1.0, 6.0),
        ],
        extraction: None,
        enemy_spawns: vec![],
        grid_cells: vec![],
    }
}


// ---------------------------------------------------------------------------
// Legacy level builders (village sim, greybox — not used in extraction mode)
// ---------------------------------------------------------------------------

/// Four walls with a door gap facing the square, for the village buildings.
fn building_walls(center: Vec3, size: Vec2, height: f32) -> Vec<StaticDef> {
    const T: f32 = 0.3;
    const DOOR: f32 = 1.6;
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    let side = crate::village_map::door_side(center, size);
    let door_on_x = side.x != 0.0;
    let mut walls = Vec::new();
    let mut solid = |position: Vec3, size: Vec3| {
        walls.push(StaticDef::axis_aligned(StaticKind::Building, position, size));
    };
    let y = height / 2.0;
    if door_on_x {
        let door_x = side.x * hx;
        let long = size.y + T;
        solid(center + Vec3::new(-door_x, y, 0.0), Vec3::new(T, height, long));
        let seg = (long - DOOR) / 2.0;
        solid(
            center + Vec3::new(door_x, y, -(DOOR / 2.0 + seg / 2.0)),
            Vec3::new(T, height, seg),
        );
        solid(
            center + Vec3::new(door_x, y, DOOR / 2.0 + seg / 2.0),
            Vec3::new(T, height, seg),
        );
        solid(center + Vec3::new(0.0, y, -hz), Vec3::new(size.x - T, height, T));
        solid(center + Vec3::new(0.0, y, hz), Vec3::new(size.x - T, height, T));
    } else {
        let door_z = side.z * hz;
        let long = size.x + T;
        solid(center + Vec3::new(0.0, y, -door_z), Vec3::new(long, height, T));
        let seg = (long - DOOR) / 2.0;
        solid(
            center + Vec3::new(-(DOOR / 2.0 + seg / 2.0), y, door_z),
            Vec3::new(seg, height, T),
        );
        solid(
            center + Vec3::new(DOOR / 2.0 + seg / 2.0, y, door_z),
            Vec3::new(seg, height, T),
        );
        solid(center + Vec3::new(-hx, y, 0.0), Vec3::new(T, height, size.y - T));
        solid(center + Vec3::new(hx, y, 0.0), Vec3::new(T, height, size.y - T));
    }
    walls
}

fn building_roof(center: Vec3, size: Vec2, height: f32) -> Vec<StaticDef> {
    const OVERHANG: f32 = 0.45;
    const PITCH: f32 = 0.62;
    const SLAB: f32 = 0.14;
    let ridge_x = size.x >= size.y;
    let (long, short) = if ridge_x { (size.x, size.y) } else { (size.y, size.x) };
    let span = short / 2.0 + OVERHANG;
    let rise = span * PITCH.tan();
    let length = long + 2.0 * OVERHANG;
    let width = span / PITCH.cos() + 0.12;
    let y = height + rise / 2.0;

    let mut parts = Vec::new();
    for side in [-1.0f32, 1.0] {
        let (position, rotation, slab_size) = if ridge_x {
            (
                center + Vec3::new(0.0, y, side * span / 2.0),
                Quat::from_rotation_x(side * PITCH),
                Vec3::new(length, SLAB, width),
            )
        } else {
            (
                center + Vec3::new(side * span / 2.0, y, 0.0),
                Quat::from_rotation_z(-side * PITCH),
                Vec3::new(width, SLAB, length),
            )
        };
        parts.push(StaticDef { kind: StaticKind::Roof, position, rotation, size: slab_size });
    }
    for side in [-1.0f32, 1.0] {
        let (position, rotation) = if ridge_x {
            (
                center + Vec3::new(side * size.x / 2.0, height, 0.0),
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            )
        } else {
            (center + Vec3::new(0.0, height, side * size.y / 2.0), Quat::IDENTITY)
        };
        parts.push(StaticDef {
            kind: StaticKind::Gable,
            position,
            rotation,
            size: Vec3::new(short, rise, 0.3),
        });
    }
    parts
}

fn building(center: Vec3, size: Vec2, height: f32) -> Vec<StaticDef> {
    let mut parts = building_walls(center, size, height);
    parts.extend(building_roof(center, size, height));
    parts
}

pub fn village_level() -> LevelDef {
    use crate::village_map::{building_size, home_world_pos, place_world_pos};

    let square = place_world_pos("square");
    let mut statics = vec![
        StaticDef::axis_aligned(
            StaticKind::Square,
            square + Vec3::new(0.0, 0.01, 0.0),
            Vec3::new(12.0, 0.04, 12.0),
        ),
        StaticDef::axis_aligned(
            StaticKind::Field,
            place_world_pos("farm") + Vec3::new(0.0, 0.01, 0.0),
            Vec3::new(18.0, 0.04, 14.0),
        ),
        StaticDef::axis_aligned(
            StaticKind::Pier,
            place_world_pos("dock") + Vec3::new(0.0, 0.05, 0.0),
            Vec3::new(4.0, 0.2, 10.0),
        ),
    ];
    for place in ["tavern", "bakery"] {
        statics.extend(building(place_world_pos(place), building_size(place), 3.0));
    }
    statics.extend(building(
        place_world_pos("farm") + Vec3::new(0.0, 0.0, -9.0),
        Vec2::new(5.0, 4.0),
        2.8,
    ));
    for index in 0..8 {
        let home = home_world_pos(index);
        statics.extend(building(home, building_size("home"), 2.5));
    }

    let props = vec![
        PropDef {
            shape: PropShape::Crate { size: Vec3::splat(0.8) },
            position: Vec3::new(3.0, 0.4, -4.0),
            density: 60.0,
        },
        PropDef {
            shape: PropShape::Crate { size: Vec3::splat(0.8) },
            position: Vec3::new(3.0, 1.25, -4.0),
            density: 60.0,
        },
        PropDef {
            shape: PropShape::Ball { radius: 0.4 },
            position: Vec3::new(-3.5, 0.5, -4.0),
            density: 60.0,
        },
    ];

    LevelDef {
        id: "village".into(),
        statics,
        props,
        lights: vec![],
        item_spawns: vec![
            Vec3::new(2.0, 0.6, 2.0),
            place_world_pos("dock") + Vec3::new(0.0, 0.6, 4.0),
            place_world_pos("farm") + Vec3::new(4.0, 0.6, 4.0),
        ],
        player_spawns: vec![
            Vec3::new(0.0, 1.0, -8.0),
            Vec3::new(2.0, 1.0, -8.0),
            Vec3::new(-2.0, 1.0, -8.0),
            Vec3::new(4.0, 1.0, -8.0),
        ],
        extraction: None,
        enemy_spawns: vec![],
        grid_cells: vec![],
    }
}

/// The Phase 1 greybox test level.
pub fn test_level() -> LevelDef {
    use StaticKind::*;

    const FLOOR_SIZE: f32 = 40.0;
    const WALL_HEIGHT: f32 = 4.0;
    const WALL_THICKNESS: f32 = 0.5;
    const HALF: f32 = FLOOR_SIZE / 2.0;

    let mut statics = vec![
        StaticDef::axis_aligned(
            Floor,
            Vec3::new(0.0, -0.25, 0.0),
            Vec3::new(FLOOR_SIZE, 0.5, FLOOR_SIZE),
        ),
        StaticDef::axis_aligned(
            Wall,
            Vec3::new(0.0, WALL_HEIGHT / 2.0, -HALF + WALL_THICKNESS / 2.0),
            Vec3::new(FLOOR_SIZE, WALL_HEIGHT, WALL_THICKNESS),
        ),
        StaticDef::axis_aligned(
            Wall,
            Vec3::new(0.0, WALL_HEIGHT / 2.0, HALF - WALL_THICKNESS / 2.0),
            Vec3::new(FLOOR_SIZE, WALL_HEIGHT, WALL_THICKNESS),
        ),
        StaticDef::axis_aligned(
            Wall,
            Vec3::new(-HALF + WALL_THICKNESS / 2.0, WALL_HEIGHT / 2.0, 0.0),
            Vec3::new(WALL_THICKNESS, WALL_HEIGHT, FLOOR_SIZE - 2.0 * WALL_THICKNESS),
        ),
        StaticDef::axis_aligned(
            Wall,
            Vec3::new(HALF - WALL_THICKNESS / 2.0, WALL_HEIGHT / 2.0, 0.0),
            Vec3::new(WALL_THICKNESS, WALL_HEIGHT, FLOOR_SIZE - 2.0 * WALL_THICKNESS),
        ),
    ];

    const DOOR_WIDTH: f32 = 2.0;
    const DOOR_HEIGHT: f32 = 3.0;
    const DIVIDER_Z: f32 = 5.0;
    let segment_len = HALF - DOOR_WIDTH / 2.0;
    statics.push(StaticDef::axis_aligned(
        Wall,
        Vec3::new(-(DOOR_WIDTH / 2.0 + segment_len / 2.0), WALL_HEIGHT / 2.0, DIVIDER_Z),
        Vec3::new(segment_len, WALL_HEIGHT, WALL_THICKNESS),
    ));
    statics.push(StaticDef::axis_aligned(
        Wall,
        Vec3::new(DOOR_WIDTH / 2.0 + segment_len / 2.0, WALL_HEIGHT / 2.0, DIVIDER_Z),
        Vec3::new(segment_len, WALL_HEIGHT, WALL_THICKNESS),
    ));
    statics.push(StaticDef::axis_aligned(
        Wall,
        Vec3::new(0.0, (DOOR_HEIGHT + WALL_HEIGHT) / 2.0, DIVIDER_Z),
        Vec3::new(DOOR_WIDTH, WALL_HEIGHT - DOOR_HEIGHT, WALL_THICKNESS),
    ));

    const PLATFORM_TOP: f32 = 3.0;
    statics.push(StaticDef::axis_aligned(
        Platform,
        Vec3::new(12.0, PLATFORM_TOP / 2.0, -12.0),
        Vec3::new(8.0, PLATFORM_TOP, 8.0),
    ));

    let run = 10.0_f32;
    let rise = PLATFORM_TOP;
    let slope_len = (run * run + rise * rise).sqrt();
    let angle = rise.atan2(run);
    statics.push(StaticDef {
        kind: Ramp,
        position: Vec3::new(8.0 - run / 2.0, rise / 2.0, -12.0),
        rotation: Quat::from_rotation_z(angle),
        size: Vec3::new(slope_len, 0.5, 4.0),
    });

    let mut props = Vec::new();
    const CRATE: f32 = 0.8;
    let stack_origin = Vec3::new(-5.0, 0.0, 12.0);
    let mut layer_y = CRATE / 2.0;
    for (layer, count) in [4usize, 3, 2, 1].into_iter().enumerate() {
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * (CRATE + 0.02)
                + layer as f32 * 0.05;
            props.push(PropDef {
                shape: PropShape::Crate { size: Vec3::splat(CRATE) },
                position: stack_origin + Vec3::new(x, layer_y, 0.0),
                density: 60.0,
            });
        }
        layer_y += CRATE + 0.02;
    }
    for (radius, density, position) in [
        (0.3, 40.0, Vec3::new(2.0, 2.3, 10.0)),
        (0.5, 100.0, Vec3::new(4.0, 2.5, 10.0)),
        (0.9, 180.0, stack_origin + Vec3::new(0.3, 8.0, 0.0)),
    ] {
        props.push(PropDef {
            shape: PropShape::Ball { radius },
            position,
            density,
        });
    }

    LevelDef {
        id: "test".into(),
        statics,
        props,
        lights: vec![],
        item_spawns: vec![
            Vec3::new(-10.0, 1.0, -10.0),
            Vec3::new(5.0, 1.0, 12.0),
            Vec3::new(12.0, PLATFORM_TOP + 1.0, -12.0),
        ],
        player_spawns: vec![
            Vec3::new(-8.0, 1.0, 12.0),
            Vec3::new(8.0, 1.0, 12.0),
            Vec3::new(0.0, 1.0, 15.0),
            Vec3::new(-4.0, 1.0, 9.0),
        ],
        extraction: None,
        enemy_spawns: vec![],
        grid_cells: vec![],
    }
}

