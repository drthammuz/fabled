//! Levels are dumb data: a list of spawnable things. For now they are built
//! by Rust functions; later they can be parsed from a file (e.g. a
//! TrenchBroom .map) into the same `LevelDef` without touching gameplay code.

use bevy::prelude::*;

use crate::props::PropShape;

/// A complete level description. Gameplay code consumes this; it never
/// cares where it came from.
pub struct LevelDef {
    pub statics: Vec<StaticDef>,
    pub props: Vec<PropDef>,
    pub item_spawns: Vec<Vec3>,
    pub player_spawns: Vec<Vec3>,
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
    /// Flat cosmetic patches (farm field, the square); thin, walkable.
    Field,
    /// Dock planks over the water line.
    Pier,
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

/// The level every run mode currently loads.
pub fn active_level() -> LevelDef {
    village_level()
}

/// Four walls with a door gap facing `door_toward` (axis-aligned, whichever
/// axis dominates), for the village buildings.
fn building_walls(center: Vec3, size: Vec2, height: f32, door_toward: Vec3) -> Vec<StaticDef> {
    const T: f32 = 0.3; // wall thickness
    const DOOR: f32 = 1.6;
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    let to = door_toward - center;
    // Which wall holds the door?
    let door_on_x = to.x.abs() * hz > to.z.abs() * hx; // compare normalized
    let mut walls = Vec::new();
    let mut solid = |position: Vec3, size: Vec3| {
        walls.push(StaticDef::axis_aligned(StaticKind::Building, position, size));
    };
    let y = height / 2.0;
    // The two walls parallel to the door wall axis.
    if door_on_x {
        let door_x = if to.x > 0.0 { hx } else { -hx };
        // Solid wall opposite the door.
        solid(center + Vec3::new(-door_x, y, 0.0), Vec3::new(T, height, size.y));
        // Door wall: two segments above/below the gap.
        let seg = (size.y - DOOR) / 2.0;
        solid(
            center + Vec3::new(door_x, y, -(DOOR / 2.0 + seg / 2.0)),
            Vec3::new(T, height, seg),
        );
        solid(
            center + Vec3::new(door_x, y, DOOR / 2.0 + seg / 2.0),
            Vec3::new(T, height, seg),
        );
        // Side walls.
        solid(center + Vec3::new(0.0, y, -hz), Vec3::new(size.x, height, T));
        solid(center + Vec3::new(0.0, y, hz), Vec3::new(size.x, height, T));
    } else {
        let door_z = if to.z > 0.0 { hz } else { -hz };
        solid(center + Vec3::new(0.0, y, -door_z), Vec3::new(size.x, height, T));
        let seg = (size.x - DOOR) / 2.0;
        solid(
            center + Vec3::new(-(DOOR / 2.0 + seg / 2.0), y, door_z),
            Vec3::new(seg, height, T),
        );
        solid(
            center + Vec3::new(DOOR / 2.0 + seg / 2.0, y, door_z),
            Vec3::new(seg, height, T),
        );
        solid(center + Vec3::new(-hx, y, 0.0), Vec3::new(T, height, size.y));
        solid(center + Vec3::new(hx, y, 0.0), Vec3::new(T, height, size.y));
    }
    walls
}

/// The village: open ground, public buildings around the square, a ring of
/// huts, the farm field out west and the dock down south-east.
pub fn village_level() -> LevelDef {
    use crate::village_map::{building_size, home_world_pos, place_world_pos};

    const GROUND: f32 = 160.0;
    let square = place_world_pos("square");

    let mut statics = vec![
        // Ground: top surface at y = 0.
        StaticDef::axis_aligned(
            StaticKind::Floor,
            Vec3::new(0.0, -0.25, 0.0),
            Vec3::new(GROUND, 0.5, GROUND),
        ),
        // The village square: a slightly raised cobble patch.
        StaticDef::axis_aligned(
            StaticKind::Field,
            square + Vec3::new(0.0, 0.01, 0.0),
            Vec3::new(12.0, 0.04, 12.0),
        ),
        // Farm field.
        StaticDef::axis_aligned(
            StaticKind::Field,
            place_world_pos("farm") + Vec3::new(0.0, 0.01, 0.0),
            Vec3::new(18.0, 0.04, 14.0),
        ),
        // Dock pier.
        StaticDef::axis_aligned(
            StaticKind::Pier,
            place_world_pos("dock") + Vec3::new(0.0, 0.05, 0.0),
            Vec3::new(4.0, 0.2, 10.0),
        ),
    ];

    // Public buildings.
    for place in ["tavern", "bakery"] {
        statics.extend(building_walls(
            place_world_pos(place),
            building_size(place),
            3.0,
            square,
        ));
    }
    // Farm barn (sits at the edge of the field).
    statics.extend(building_walls(
        place_world_pos("farm") + Vec3::new(0.0, 0.0, -9.0),
        Vec2::new(5.0, 4.0),
        2.8,
        square,
    ));
    // Villager huts.
    for index in 0..8 {
        let home = home_world_pos(index);
        statics.extend(building_walls(home, building_size("home"), 2.5, square));
    }

    // A few crates and a ball near the square so the physics toys remain.
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
        statics,
        props,
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
    }
}

/// The Phase 1 greybox test level.
///
/// Layout (top-down, +x right, +z down on this sketch):
/// - 40x40 floor with 4 m perimeter walls.
/// - A dividing wall at z = 5 splits it into a north room and a south room,
///   connected by a doorway (2 m wide, 3 m tall) at x = 0.
/// - North room: raised platform (top at y = 3) with a ramp up from the west.
pub fn test_level() -> LevelDef {
    use StaticKind::*;

    const FLOOR_SIZE: f32 = 40.0;
    const WALL_HEIGHT: f32 = 4.0;
    const WALL_THICKNESS: f32 = 0.5;
    const HALF: f32 = FLOOR_SIZE / 2.0;

    let mut statics = vec![
        // Floor: top surface at y = 0.
        StaticDef::axis_aligned(
            Floor,
            Vec3::new(0.0, -0.25, 0.0),
            Vec3::new(FLOOR_SIZE, 0.5, FLOOR_SIZE),
        ),
        // Perimeter walls (north/south span full width, east/west fit between).
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

    // Dividing wall at z = 5 with a doorway at x = 0 (2 m wide, 3 m tall).
    const DOOR_WIDTH: f32 = 2.0;
    const DOOR_HEIGHT: f32 = 3.0;
    const DIVIDER_Z: f32 = 5.0;
    let segment_len = HALF - DOOR_WIDTH / 2.0; // from doorway edge to outer wall
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
    // Lintel above the doorway.
    statics.push(StaticDef::axis_aligned(
        Wall,
        Vec3::new(0.0, (DOOR_HEIGHT + WALL_HEIGHT) / 2.0, DIVIDER_Z),
        Vec3::new(DOOR_WIDTH, WALL_HEIGHT - DOOR_HEIGHT, WALL_THICKNESS),
    ));

    // Raised platform in the north room: 8x8, top surface at y = 3.
    const PLATFORM_TOP: f32 = 3.0;
    statics.push(StaticDef::axis_aligned(
        Platform,
        Vec3::new(12.0, PLATFORM_TOP / 2.0, -12.0),
        Vec3::new(8.0, PLATFORM_TOP, 8.0),
    ));

    // Ramp up to the platform from the west: runs 10 m, rises 3 m.
    let run = 10.0_f32;
    let rise = PLATFORM_TOP;
    let slope_len = (run * run + rise * rise).sqrt();
    let angle = rise.atan2(run); // positive z-rotation lifts the +x end
    statics.push(StaticDef {
        kind: Ramp,
        position: Vec3::new(8.0 - run / 2.0, rise / 2.0, -12.0),
        rotation: Quat::from_rotation_z(angle),
        size: Vec3::new(slope_len, 0.5, 4.0),
    });

    // Dynamic props: a stack of 10 crates in the south room plus balls of
    // different sizes/masses. The stack is a 4-3-2-1 pyramid with slight
    // x-offsets so it tumbles interestingly.
    let mut props = Vec::new();
    const CRATE: f32 = 0.8;
    let stack_origin = Vec3::new(-5.0, 0.0, 12.0);
    let mut layer_y = CRATE / 2.0;
    for (layer, count) in [4usize, 3, 2, 1].into_iter().enumerate() {
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * (CRATE + 0.02)
                + layer as f32 * 0.05;
            props.push(PropDef {
                shape: PropShape::Crate {
                    size: Vec3::splat(CRATE),
                },
                position: stack_origin + Vec3::new(x, layer_y, 0.0),
                density: 60.0,
            });
        }
        layer_y += CRATE + 0.02;
    }
    // Light and medium balls drop harmlessly nearby; the heavy boulder
    // drops straight onto the crate pyramid to demonstrate mass differences.
    for (radius, density, position) in [
        (0.3, 40.0, Vec3::new(2.0, 2.3, 10.0)),   // light beach ball
        (0.5, 100.0, Vec3::new(4.0, 2.5, 10.0)),  // medium
        (0.9, 180.0, stack_origin + Vec3::new(0.3, 8.0, 0.0)), // heavy boulder
    ] {
        props.push(PropDef {
            shape: PropShape::Ball { radius },
            position,
            density,
        });
    }

    LevelDef {
        statics,
        props,
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
    }
}
