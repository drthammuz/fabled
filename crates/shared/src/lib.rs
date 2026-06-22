pub mod classes;
pub mod config;
pub mod editor_catalog;
pub mod editor_map;
pub mod editor_settings;
pub mod hidden_door;
pub mod items;
pub mod map_pool;
pub mod kenney_catalog;
pub mod kenney_hub;
pub mod kenney_layout;
pub mod kenney_pit;
pub mod kenney_transitions;
pub mod level;
pub mod props;
pub mod protocol;
pub mod run;
pub mod terrain;
pub mod village_map;

/// How the developer test map is built when `--test` is active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TestMapStyle {
    /// Procedural walls/floors/stairs from `userinput/wall_map.json` (cuboids).
    #[default]
    Rusty,
    /// Kenney Modular Space Kit GLBs (`test_showcase`) for interior pieces.
    Kenney,
}

/// Incremented when editor playtest toggles; server rebuilds Kenney colliders + floors.
#[derive(bevy::prelude::Resource, Default)]
pub struct KenneyPlaytestGeneration(pub u32);

/// Present when running `--host --editor` (Kenney layout tool).
#[derive(bevy::prelude::Resource, Clone, Copy, Default)]
pub struct EditorMode;

/// Present when running `--city` (standalone GLB viewer, no gameplay).
#[derive(bevy::prelude::Resource, Clone, Copy, Default)]
pub struct CityViewMode;

/// Present (inserted by `--host --test`) when running the developer test map:
/// procgen + the class-select screen are bypassed, the flat `testmap` level is
/// loaded, and classes are auto-picked. Absent in normal play. Removing this
/// resource (don't pass `--test`) restores the full game flow.
#[derive(bevy::prelude::Resource, Clone, Copy, Default)]
pub struct TestMode {
    pub style: TestMapStyle,
}
