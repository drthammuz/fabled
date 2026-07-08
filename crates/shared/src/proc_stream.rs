//! Runtime hub map streaming state (shared between server + client systems).
//!
//! Two procgen child maps are generated in the background while the player
//! plays the active map, mounted under the active hub's two drop exits, and
//! the player COMMITS by dropping into one: the chosen child becomes the new
//! active map (re-based to the world origin), the parent and the unchosen
//! sibling despawn, and two new children start generating. Only three maps
//! ever exist at once — an endless run with no loading screens.
//!
//! Faction chain: a child's start (prev) faction = the parent's exit (next)
//! faction; the child's own exit faction is randomized.
//!
//! v1 scope: editor playtest (G) on a listen server / solo host. The client
//! owns generation + commit (it has the editor workspace); the server owns
//! child colliders (it has the trimesh bake pipeline). Both talk through this
//! resource — in host mode they share one Bevy `World`.

use bevy::prelude::*;

use crate::kenney_layout::KenneyLayout;

/// Where background-generated child map docs are written.
pub const CHILD_MAP_DIR: &str = "userinput/maps/stream";

/// Ground factions eligible as a child map's randomized exit faction.
pub const CHAIN_FACTIONS: [&str; 4] = ["outlaw", "priesthood", "synth", "necropolis"];

/// Generator knobs mirrored from the editor's Proc panel for child maps.
#[derive(Clone, Debug)]
pub struct GenKnobs {
    pub cells: u32,
    pub rooms: u32,
    pub loops: u32,
    pub attempts: u32,
    pub organicness: f32,
    pub corridor_width: f32,
    pub hidden: f32,
    pub prev_fraction: f32,
    pub default_fraction: f32,
    pub next_fraction: f32,
    pub enemy_count: u32,
}

impl Default for GenKnobs {
    fn default() -> Self {
        // Editor Proc-panel defaults (MapGenSettings::default).
        Self {
            cells: 25,
            rooms: 11,
            loops: 3,
            attempts: 30,
            organicness: 0.0,
            corridor_width: 1.0,
            hidden: 1.0,
            prev_fraction: 15.0 / 110.0,
            default_fraction: 60.0 / 110.0,
            next_fraction: 35.0 / 110.0,
            enemy_count: 5,
        }
    }
}

/// One generated child map mounted under an active hub exit.
#[derive(Clone)]
pub struct ProcChild {
    /// Active layout `hub_exits` key ("0" | "1").
    pub exit_key: String,
    /// Generated map doc on disk (map-local coordinates).
    pub doc_path: std::path::PathBuf,
    /// Parsed layout (local coords; offset NOT applied).
    pub layout: KenneyLayout,
    /// World offset so the child's spawn sits under the hub exit hole.
    pub offset: Vec3,
    /// Tag for spawned colliders/visuals (active map is 1).
    pub instance_id: u32,
    /// This child's exit faction — becomes the chain source when committed.
    pub next_faction: String,
    /// Server has spawned colliders + floor cells for this child.
    pub colliders_spawned: bool,
    /// Client has spawned piece visuals for this child.
    pub visuals_spawned: bool,
}

#[derive(Resource, Default)]
pub struct ProcStreamState {
    /// Exit (next) faction of the ACTIVE map — the chain source for children.
    pub active_next_faction: String,
    /// Mounted children awaiting a commit (0–2).
    pub children: Vec<ProcChild>,
    /// Set by commit detection; consumed by the client commit executor.
    pub committed: Option<ProcChild>,
    /// Increments per active-map swap (names child files, invalidates jobs).
    pub round: u32,
}
