//! Declarative floor transitions between vertical bands.
//!
//! Map authoring can specify transitions in module-local tile coordinates (NW corner
//! of the 5×5 module grid is `(0, 0)`, `x` increases east, `z` increases south):
//!
//! - **Stair up** `(bottom) -> (top)` from floor `a` to `b`: lowest step on `bottom`,
//!   run ends on `top`. Omits `template-floor` on **both** tiles on **both** `a` and
//!   `b` so the stair mesh is the only walk surface (no double texture / no ceiling lip).
//! - **Trap down** `(tile)` from floor `a` to `b`: omits floor on `a` only; landing on
//!   `b` at the same tile stays solid (place `template-floor-hole` frame separately).

use bevy::prelude::Vec2;

use crate::editor_map::CELLS_PER_MODULE;
use crate::kenney_catalog::KENNEY_CELL;
use crate::kenney_pit::{DEPTH_FLOOR_LEVEL, HUB_FLOOR_LEVEL};

/// One cell in a 5×5 Kenney module (`ix` west→east, `iz` south→north; NW = `(0, 0)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleTile {
    pub ix: u32,
    pub iz: u32,
}

impl ModuleTile {
    pub const fn new(ix: u32, iz: u32) -> Self {
        Self { ix, iz }
    }

    /// Metres from module centre to this cell centre (matches `tools/gen_modules.cell_cx/cz`).
    pub fn offset_m(self) -> (f32, f32) {
        let c = CELLS_PER_MODULE as f32 * 0.5 - 0.5;
        (
            (self.ix as f32 - c) * KENNEY_CELL,
            (self.iz as f32 - c) * KENNEY_CELL,
        )
    }

    pub fn world_xz(self, mcx: f32, mcz: f32) -> (f32, f32) {
        let (dx, dz) = self.offset_m();
        (mcx + dx, mcz + dz)
    }
}

/// Walk **up** from `from_floor` to `to_floor` across `bottom` → `top` module tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StairUp {
    pub bottom: ModuleTile,
    pub top: ModuleTile,
    pub from_floor: i32,
    pub to_floor: i32,
}

impl StairUp {
    pub const fn new(bottom: ModuleTile, top: ModuleTile, from_floor: i32, to_floor: i32) -> Self {
        Self {
            bottom,
            top,
            from_floor,
            to_floor,
        }
    }

    /// Both cells the stair footprint occupies (entry + landing).
    pub fn footprint(self) -> [ModuleTile; 2] {
        [self.bottom, self.top]
    }

    /// `(floor, world_x, world_z)` for every cell that must **not** carry `template-floor`.
    pub fn floor_omit_world(self, mcx: f32, mcz: f32) -> [(i32, f32, f32); 4] {
        let b = self.bottom.world_xz(mcx, mcz);
        let t = self.top.world_xz(mcx, mcz);
        [
            (self.from_floor, b.0, b.1),
            (self.from_floor, t.0, t.1),
            (self.to_floor, b.0, b.1),
            (self.to_floor, t.0, t.1),
        ]
    }

    /// World XZ of the upper landing cell (hole on `to_floor`).
    pub fn top_world(self, mcx: f32, mcz: f32) -> Vec2 {
        let (x, z) = self.top.world_xz(mcx, mcz);
        Vec2::new(x, z)
    }

    /// World XZ of both cells (for mask cuts / probes).
    pub fn footprint_world(self, mcx: f32, mcz: f32) -> [Vec2; 2] {
        let b = self.bottom.world_xz(mcx, mcz);
        let t = self.top.world_xz(mcx, mcz);
        [Vec2::new(t.0, t.1), Vec2::new(b.0, b.1)]
    }
}

/// Drop through a single tile from `from_floor` to `to_floor` (trap / pit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrapDown {
    pub tile: ModuleTile,
    pub from_floor: i32,
    pub to_floor: i32,
}

impl TrapDown {
    pub const fn new(tile: ModuleTile, from_floor: i32, to_floor: i32) -> Self {
        Self {
            tile,
            from_floor,
            to_floor,
        }
    }

    /// Only the upper floor tile is open; landing on `to_floor` stays solid.
    pub fn upper_hole_world(self, mcx: f32, mcz: f32) -> (f32, f32) {
        self.tile.world_xz(mcx, mcz)
    }
}

/// Hub L2 stairs in the west module: `(2,2) -> (3,2)` from depth `-2` to hub `-1`.
/// Entry / lowest step on centre tile; run climbs one cell east toward the door.
pub fn hub_l2_stair_up() -> StairUp {
    StairUp::new(
        ModuleTile::new(2, 2),
        ModuleTile::new(3, 2),
        DEPTH_FLOOR_LEVEL,
        HUB_FLOOR_LEVEL,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_stair_footprint_matches_legacy_offsets() {
        let stair = hub_l2_stair_up();
        let wx = 20.0;
        let wz = 40.0;
        let cells = stair.footprint_world(wx, wz);
        assert_eq!(cells[0], Vec2::new(24.0, 40.0));
        assert_eq!(cells[1], Vec2::new(20.0, 40.0));
        let holes = stair.floor_omit_world(wx, wz);
        assert_eq!(holes.len(), 4);
        assert!(holes.iter().any(|(f, x, z)| *f == DEPTH_FLOOR_LEVEL && *x == 20.0 && *z == 40.0));
        assert!(holes.iter().any(|(f, x, z)| *f == DEPTH_FLOOR_LEVEL && *x == 24.0 && *z == 40.0));
    }
}
