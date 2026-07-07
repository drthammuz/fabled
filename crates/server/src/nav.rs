//! Baked-grid navigation for server agents (enemies/NPCs).
//!
//! The generator writes a nav grid into each proc map (`gen_freeform.build_nav_grid`):
//! walkable 4 m cells with per-cell floor Y and the passable faces (walls block,
//! doors pass, 1.2 m elevation steps only via stair cells). Those are the exact
//! rules that guarantee PLAYER reachability, so any path found here is walkable
//! in-world. A* runs over that graph; local steering + gravity do the rest.

use std::collections::{BinaryHeap, HashMap};

use bevy::prelude::*;
use shared::kenney_layout::KenneyLayout;

const OPEN_N: u8 = 1;
const OPEN_S: u8 = 2;
const OPEN_E: u8 = 4;
const OPEN_W: u8 = 8;

/// (side bit, dix, diz) — grid N is -z, S is +z (matches the generator).
const SIDES: [(u8, i32, i32); 4] = [
    (OPEN_N, 0, -1),
    (OPEN_S, 0, 1),
    (OPEN_E, 1, 0),
    (OPEN_W, -1, 0),
];

#[derive(Clone, Copy)]
pub struct NavCellData {
    /// Floor surface Y of the cell (zone elevation).
    pub y: f32,
    /// Passable faces (OPEN_* bits).
    pub open: u8,
}

/// Nav grid of the active map. Empty (no cells) on maps without baked nav
/// (hub, testmap) — agents then fall back to local wander.
#[derive(Resource, Default)]
pub struct EnemyNav {
    pub cell_m: f32,
    pub cells_x: u32,
    pub cells_z: u32,
    pub cells: HashMap<(i32, i32), NavCellData>,
}

impl EnemyNav {
    pub fn from_layout(layout: &KenneyLayout) -> Self {
        let Some(nav) = layout.nav.as_ref() else {
            return Self::default();
        };
        let mut cells = HashMap::with_capacity(nav.cells.len());
        for c in &nav.cells {
            let mut open = 0u8;
            for ch in c.open.chars() {
                open |= match ch {
                    'N' => OPEN_N,
                    'S' => OPEN_S,
                    'E' => OPEN_E,
                    'W' => OPEN_W,
                    _ => 0,
                };
            }
            cells.insert((c.c[0], c.c[1]), NavCellData { y: c.y, open });
        }
        Self {
            cell_m: nav.cell_m.max(0.1),
            cells_x: nav.cells_x,
            cells_z: nav.cells_z,
            cells,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn cell_of(&self, p: Vec3) -> (i32, i32) {
        (
            (p.x / self.cell_m + self.cells_x as f32 / 2.0 - 0.5).round() as i32,
            (p.z / self.cell_m + self.cells_z as f32 / 2.0 - 0.5).round() as i32,
        )
    }

    /// Cell centre at floor height.
    pub fn world_of(&self, c: (i32, i32)) -> Vec3 {
        let y = self.cells.get(&c).map(|d| d.y).unwrap_or(0.0);
        Vec3::new(
            (c.0 as f32 - self.cells_x as f32 / 2.0 + 0.5) * self.cell_m,
            y,
            (c.1 as f32 - self.cells_z as f32 / 2.0 + 0.5) * self.cell_m,
        )
    }

    /// The agent's cell, or the nearest nav cell when it stands off-grid.
    pub fn nearest_cell(&self, p: Vec3) -> Option<(i32, i32)> {
        let c = self.cell_of(p);
        if self.cells.contains_key(&c) {
            return Some(c);
        }
        self.cells
            .keys()
            .min_by(|a, b| {
                let da = (a.0 - c.0).pow(2) + (a.1 - c.1).pow(2);
                let db = (b.0 - c.0).pow(2) + (b.1 - c.1).pow(2);
                da.cmp(&db)
            })
            .copied()
    }

    /// A* over open faces; result includes `from` and `to`.
    pub fn find_path(&self, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        if !self.cells.contains_key(&from) || !self.cells.contains_key(&to) {
            return None;
        }
        if from == to {
            return Some(vec![from]);
        }

        #[derive(PartialEq)]
        struct Node(f32, (i32, i32));
        impl Eq for Node {}
        impl Ord for Node {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // min-heap on f
                other.0.partial_cmp(&self.0).unwrap_or(std::cmp::Ordering::Equal)
            }
        }
        impl PartialOrd for Node {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        let h = |c: (i32, i32)| ((c.0 - to.0).abs() + (c.1 - to.1).abs()) as f32;
        let mut open = BinaryHeap::new();
        let mut g: HashMap<(i32, i32), f32> = HashMap::new();
        let mut came: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        g.insert(from, 0.0);
        open.push(Node(h(from), from));

        while let Some(Node(_, c)) = open.pop() {
            if c == to {
                let mut path = vec![c];
                let mut cur = c;
                while let Some(&prev) = came.get(&cur) {
                    path.push(prev);
                    cur = prev;
                }
                path.reverse();
                return Some(path);
            }
            let cd = self.cells[&c];
            for (bit, dx, dz) in SIDES {
                if cd.open & bit == 0 {
                    continue;
                }
                let nb = (c.0 + dx, c.1 + dz);
                let Some(nd) = self.cells.get(&nb) else {
                    continue;
                };
                let ng = g[&c] + 1.0 + (nd.y - cd.y).abs();
                if ng < *g.get(&nb).unwrap_or(&f32::INFINITY) {
                    g.insert(nb, ng);
                    came.insert(nb, c);
                    open.push(Node(ng + h(nb), nb));
                }
            }
        }
        None
    }

    /// Nearby cell that offers cover from a threat: within `max_steps` of
    /// `from` (reachable via open faces) and whose face toward the threat is
    /// WALLED, so the agent can duck behind level geometry. Prefers the
    /// closest such cell; never returns `from` itself.
    pub fn cover_cell_from(
        &self,
        from: (i32, i32),
        threat: Vec3,
        max_steps: u32,
    ) -> Option<(i32, i32)> {
        if !self.cells.contains_key(&from) {
            return None;
        }
        let covered = |c: (i32, i32)| -> bool {
            let d = threat - self.world_of(c);
            // Dominant horizontal axis toward the threat = the face that must be walled.
            let bit = if d.x.abs() >= d.z.abs() {
                if d.x > 0.0 { OPEN_E } else { OPEN_W }
            } else if d.z > 0.0 {
                OPEN_S
            } else {
                OPEN_N
            };
            self.cells.get(&c).is_some_and(|cd| cd.open & bit == 0)
        };
        let mut seen = vec![from];
        let mut frontier = vec![from];
        for _ in 0..max_steps {
            let mut next = Vec::new();
            for &c in &frontier {
                let cd = self.cells[&c];
                for (bit, dx, dz) in SIDES {
                    if cd.open & bit == 0 {
                        continue;
                    }
                    let nb = (c.0 + dx, c.1 + dz);
                    if self.cells.contains_key(&nb) && !seen.contains(&nb) {
                        seen.push(nb);
                        next.push(nb);
                    }
                }
            }
            // BFS order = nearest first: return the first covered cell of this ring.
            if let Some(&c) = next.iter().find(|&&c| covered(c)) {
                return Some(c);
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        None
    }

    /// Random reachable cell within `max_steps` of `from` (BFS ring), for wandering.
    pub fn random_nearby_cell(&self, from: (i32, i32), max_steps: u32, seed: u32) -> Option<(i32, i32)> {
        if !self.cells.contains_key(&from) {
            return None;
        }
        let mut seen = vec![from];
        let mut frontier = vec![from];
        for _ in 0..max_steps {
            let mut next = Vec::new();
            for &c in &frontier {
                let cd = self.cells[&c];
                for (bit, dx, dz) in SIDES {
                    if cd.open & bit == 0 {
                        continue;
                    }
                    let nb = (c.0 + dx, c.1 + dz);
                    if self.cells.contains_key(&nb) && !seen.contains(&nb) {
                        seen.push(nb);
                        next.push(nb);
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }
        if seen.len() <= 1 {
            return None;
        }
        // skip index 0 (= from) so wander always moves somewhere
        let idx = 1 + (pseudo_rand_u32(seed) as usize) % (seen.len() - 1);
        Some(seen[idx])
    }
}

fn pseudo_rand_u32(seed: u32) -> u32 {
    let mut x = seed;
    x ^= x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x ^= x.wrapping_mul(0xc2b2_ae35);
    x ^= x >> 16;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::kenney_layout::{NavCell, NavGrid};

    fn grid(cells: Vec<((i32, i32), f32, &str)>) -> EnemyNav {
        let nav = NavGrid {
            cell_m: 4.0,
            cells_x: 8,
            cells_z: 8,
            cells: cells
                .into_iter()
                .map(|((ix, iz), y, open)| NavCell {
                    c: [ix, iz],
                    y,
                    open: open.into(),
                    stair: false,
                })
                .collect(),
        };
        let layout = KenneyLayout {
            nav: Some(nav),
            ..Default::default()
        };
        EnemyNav::from_layout(&layout)
    }

    #[test]
    fn astar_routes_around_walls() {
        // 3 cells in a row, but the middle face is walled: must go around via row 1.
        let nav = grid(vec![
            ((0, 0), 0.0, "S"),      // wall to the E
            ((1, 0), 0.0, "S"),      // wall to W and E
            ((2, 0), 0.0, "S"),
            ((0, 1), 0.0, "NE"),
            ((1, 1), 0.0, "NEW"),
            ((2, 1), 0.0, "NW"),
        ]);
        let path = nav.find_path((0, 0), (2, 0)).expect("path");
        assert_eq!(path.first(), Some(&(0, 0)));
        assert_eq!(path.last(), Some(&(2, 0)));
        assert!(path.contains(&(1, 1)), "must detour through open row: {path:?}");
    }

    #[test]
    fn astar_fails_when_sealed() {
        let nav = grid(vec![((0, 0), 0.0, ""), ((1, 0), 0.0, "")]);
        assert!(nav.find_path((0, 0), (1, 0)).is_none());
    }

    #[test]
    fn cover_cell_picks_walled_face_toward_threat() {
        // Two-cell corridor running E-W; the east cell has a wall on its E face.
        // A threat to the far east should pick that east cell as cover.
        let nav = grid(vec![
            ((0, 0), 0.0, "E"), // open toward (1,0), walled elsewhere
            ((1, 0), 0.0, "W"), // open back toward (0,0); E face walled
        ]);
        let threat = nav.world_of((1, 0)) + Vec3::new(20.0, 0.0, 0.0);
        assert_eq!(nav.cover_cell_from((0, 0), threat, 3), Some((1, 0)));
        // Threat from the WEST: (1,0)'s W face is open, no cover available.
        let threat_w = nav.world_of((0, 0)) - Vec3::new(20.0, 0.0, 0.0);
        assert_eq!(nav.cover_cell_from((0, 0), threat_w, 3), None);
    }

    #[test]
    fn world_cell_roundtrip() {
        let nav = grid(vec![((3, 5), 1.2, "")]);
        let w = nav.world_of((3, 5));
        assert_eq!(nav.cell_of(w), (3, 5));
        assert!((w.y - 1.2).abs() < 1e-5);
    }
}
