//! Shared editor session state (map + module documents in memory).

use std::path::PathBuf;

use bevy::prelude::*;
use shared::editor_catalog::{self, PieceFilters};
use shared::editor_map::{
    ActiveDocKind, ActiveDocument, EditorTool, EditorWorkflow, GridSpec, MapDocument, ModuleDocument,
    SnapMode,
};

use crate::editor_sidebar::SidebarTab;

#[derive(Clone, Debug)]
pub struct NamingModal {
    pub buffer: String,
    pub replace_on_first_key: bool,
}

#[derive(Resource, Clone)]
pub struct EditorWorkspace {
    pub workflow: EditorWorkflow,
    pub tool: EditorTool,
    pub tool_before_floor: EditorTool,
    pub snap: SnapMode,
    pub floor_level: i32,
    pub file_menu_open: bool,
    pub options_menu_open: bool,
    pub actions_menu_open: bool,
    pub sidebar_collapsed: bool,
    pub sidebar_scroll: f32,
    pub sidebar_pointer_inside: bool,
    pub file_new: bool,
    pub file_discard: bool,
    pub file_save: bool,
    pub file_save_as: bool,
    pub file_load: bool,
    pub floor_paint_preview: Option<(u32, u32, u32, u32)>,
    pub floor_paint_start: Option<(u32, u32)>,
    pub floor_paint_add: bool,
    pub map: MapDocument,
    pub module: ModuleDocument,
    pub active: ActiveDocument,
    pub dirty: bool,
    pub floor_dirty: bool,
    pub floor_painting: bool,
    pub refocus_camera: bool,
    pub sidebar_tab: SidebarTab,
    pub filters: PieceFilters,
    pub sidebar_dirty: bool,
    pub selected_module: Option<PathBuf>,
    pub place_pool: String,
    pub naming_modal: Option<NamingModal>,
    pub clear_module_pieces: bool,
    pub spawn_naming_ui: bool,
    pub respawn_ghost: bool,
    pub load_module_path: Option<PathBuf>,
    pub save_as_requested: bool,
    pub pending_save_as_name: Option<String>,
    pub spawn_marker_dirty: bool,
    /// True when the cursor is over any interactive UI element (toolbar, dropdown, sidebar).
    /// Updated each frame by `update_ui_hover_block`. Guards 3-D placement in `editor_input`.
    pub pointer_over_ui: bool,
    /// Set to true to open the load-file picker modal on the next frame.
    pub pending_load_picker: bool,
    /// Set to Some(path) when the user picks a map file in the load picker.
    pub pending_load_map: Option<std::path::PathBuf>,
    /// Accumulated scroll-wheel cycles for the module picker (PlaceModule tool).
    pub module_cycle_delta: i32,
    /// Set when the naming modal is opened via "New Module" (not "Save As").
    /// When the name is confirmed, create a blank module instead of a save-as.
    pub new_module_requested: bool,
    /// Which generated pool is shown in the Gallery tab.
    pub gallery_pool: String,
    /// Index of the currently focused module in the Gallery tab.
    pub gallery_cursor: usize,
    /// Workflow that was active before entering Gallery mode (restored on exit).
    pub pre_gallery_workflow: Option<shared::editor_map::EditorWorkflow>,
}

impl Default for EditorWorkspace {
    fn default() -> Self {
        Self {
            workflow: EditorWorkflow::MapMaker,
            tool: EditorTool::PlaceGlb,
            tool_before_floor: EditorTool::PlaceGlb,
            snap: SnapMode::default(),
            floor_level: 0,
            file_menu_open: false,
            options_menu_open: false,
            actions_menu_open: false,
            sidebar_collapsed: false,
            sidebar_scroll: 0.0,
            sidebar_pointer_inside: false,
            file_new: false,
            file_discard: false,
            file_save: false,
            file_save_as: false,
            file_load: false,
            floor_paint_preview: None,
            floor_paint_start: None,
            floor_paint_add: true,
            map: MapDocument::new_default(),
            module: ModuleDocument::new_named("module_01", "default"),
            active: ActiveDocument::default(),
            dirty: false,
            floor_dirty: true,
            floor_painting: false,
            refocus_camera: false,
            sidebar_tab: SidebarTab::default(),
            filters: PieceFilters::default(),
            sidebar_dirty: true,
            selected_module: None,
            place_pool: "default".into(),
            naming_modal: None,
            clear_module_pieces: false,
            spawn_naming_ui: false,
            respawn_ghost: false,
            load_module_path: None,
            save_as_requested: false,
            pending_save_as_name: None,
            spawn_marker_dirty: false,
            pointer_over_ui: false,
            pending_load_picker: false,
            pending_load_map: None,
            module_cycle_delta: 0,
            new_module_requested: false,
            gallery_pool: "generated".into(),
            gallery_cursor: 0,
            pre_gallery_workflow: None,
        }
    }
}

impl EditorWorkspace {
    pub fn grid(&self) -> GridSpec {
        match self.workflow {
            EditorWorkflow::MapMaker => self.map.grid(),
            EditorWorkflow::ModuleMaker => {
                GridSpec::for_workflow(EditorWorkflow::ModuleMaker, 1, 1)
            }
        }
    }

    pub fn world_x0(&self) -> f32 {
        self.grid().world_x0()
    }

    pub fn sync_active_kind(&mut self) {
        self.active.kind = match self.workflow {
            EditorWorkflow::MapMaker => ActiveDocKind::Map,
            EditorWorkflow::ModuleMaker => ActiveDocKind::Module,
        };
    }

    pub fn set_workflow(&mut self, workflow: EditorWorkflow) {
        if self.workflow == workflow {
            return;
        }
        self.workflow = workflow;
        // Entering ModuleMaker: the "place module" tool and its ghost belong to MapMaker only.
        if workflow == EditorWorkflow::ModuleMaker {
            self.selected_module = None;
            if self.tool == EditorTool::PlaceModule {
                self.tool = EditorTool::PlaceGlb;
            }
            self.respawn_ghost = true;
        }
        self.sync_active_kind();
        self.floor_dirty = true;
        self.sidebar_dirty = true;
        self.file_menu_open = false;
        self.options_menu_open = false;
        self.actions_menu_open = false;
    }

    pub fn toggle_workflow(&mut self) {
        let next = match self.workflow {
            EditorWorkflow::MapMaker => EditorWorkflow::ModuleMaker,
            EditorWorkflow::ModuleMaker => EditorWorkflow::MapMaker,
        };
        self.set_workflow(next);
    }

    pub fn close_menus(&mut self) {
        self.file_menu_open = false;
        self.options_menu_open = false;
        self.actions_menu_open = false;
    }

    pub fn open_naming_modal(&mut self) {
        let suggested = editor_catalog::suggest_module_name(&self.place_pool);
        self.naming_modal = Some(NamingModal {
            buffer: suggested,
            replace_on_first_key: true,
        });
        self.spawn_naming_ui = true;
    }

    pub fn begin_new_module(&mut self, name: &str) {
        self.module = ModuleDocument::new_named(name, &self.place_pool);
        self.active.path = None;
        self.active.kind = ActiveDocKind::Module;
        self.clear_module_pieces = true;
        self.dirty = false;
        self.floor_dirty = true;
        self.refocus_camera = true;
    }

    pub fn cycle_map_modules_x(&mut self) {
        const SIZES: [u32; 6] = [3, 4, 5, 6, 7, 8];
        let i = SIZES
            .iter()
            .position(|&s| s == self.map.modules_x)
            .unwrap_or(0);
        self.map.modules_x = SIZES[(i + 1) % SIZES.len()];
        self.map.resize_map(self.map.modules_x, self.map.modules_z);
        self.floor_dirty = true;
        self.refocus_camera = true;
        self.dirty = true;
    }

    pub fn cycle_map_modules_z(&mut self) {
        const SIZES: [u32; 6] = [3, 4, 5, 6, 7, 8];
        let i = SIZES
            .iter()
            .position(|&s| s == self.map.modules_z)
            .unwrap_or(0);
        self.map.modules_z = SIZES[(i + 1) % SIZES.len()];
        self.map.resize_map(self.map.modules_x, self.map.modules_z);
        self.floor_dirty = true;
        self.refocus_camera = true;
        self.dirty = true;
    }
}

/// Marker for the spawn-point character model in the editor.
#[derive(Component)]
pub struct SpawnMarker;

#[derive(Component)]
pub struct FloorSlab;

#[derive(Component)]
pub struct FloorSlabGrid;

#[derive(Component)]
pub struct FloorSlabPainted;

#[derive(Component)]
pub struct FloorSlabPreview;

#[derive(Component)]
pub struct EditorToolbarRoot;

#[derive(Component)]
pub struct EditorMenuRoot;

#[derive(Component)]
pub struct SidebarScrollContent;

#[derive(Component)]
pub struct EditorSidebarRoot;
