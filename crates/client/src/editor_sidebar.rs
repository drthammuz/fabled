//! Right sidebar — GLB / module tabs, category filters, piece picker.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use shared::editor_catalog::{self, FilterGroup, PieceFilters};
use shared::editor_map::{EditorTool, EditorWorkflow};

use crate::editor_workspace::{EditorSidebarRoot, EditorWorkspace, SidebarScrollContent};
use crate::editor_state::EditorState;
use crate::editor_map_gen::SidebarTabGenerate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Glb,
    Module,
    Generate,
    Gallery,
}

#[derive(Component)]
pub struct SidebarPoolCycle;

#[derive(Component)]
pub struct SidebarContent;

#[derive(Component)]
pub struct SidebarTabGlb;

#[derive(Component)]
pub struct SidebarTabModule;

#[derive(Component)]
pub struct SidebarFilterAll;

#[derive(Component)]
pub struct SidebarFilterNone;

#[derive(Component)]
pub struct SidebarFilterBtn(pub FilterGroup);

#[derive(Component)]
pub struct SidebarPieceBtn(pub usize);

#[derive(Component)]
pub struct SidebarModuleBtn(pub usize);

#[derive(Component)]
pub struct SidebarTabGallery;

#[derive(Component)]
pub struct GalleryEntryBtn(pub std::path::PathBuf);

#[derive(Component)]
pub struct GalleryPoolBtn(pub String);

/// Toggle keep for a gallery module.  Inner is the module name (not path).
#[derive(Component)]
pub struct GalleryKeepBtn(pub String);

/// Toggle reject for a gallery module.
#[derive(Component)]
pub struct GalleryRejectBtn(pub String);

/// Per-session in-memory mirror of `gallery_ratings.json`.
/// true = kept, false = rejected, absent = unrated.
#[derive(Resource, Default, Clone)]
pub struct GalleryRatings {
    pub pool: String,
    pub map: std::collections::HashMap<String, bool>,
}

impl GalleryRatings {
    pub fn load(pool: &str) -> Self {
        let path = shared::editor_map::pool_dir(pool).join("gallery_ratings.json");
        let map = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { pool: pool.to_string(), map }
    }

    pub fn save(&self) {
        let path = shared::editor_map::pool_dir(&self.pool).join("gallery_ratings.json");
        if let Ok(json) = serde_json::to_string_pretty(&self.map) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Toggle keep; returns the new state (Some(true)=kept, None=cleared).
    pub fn toggle_keep(&mut self, name: &str) -> Option<bool> {
        match self.map.get(name) {
            Some(true) => { self.map.remove(name); None }
            _ => { self.map.insert(name.to_string(), true); Some(true) }
        }
    }

    /// Toggle reject.
    pub fn toggle_reject(&mut self, name: &str) -> Option<bool> {
        match self.map.get(name) {
            Some(false) => { self.map.remove(name); None }
            _ => { self.map.insert(name.to_string(), false); Some(false) }
        }
    }

    pub fn kept_count(&self)    -> usize { self.map.values().filter(|&&v| v).count() }
    pub fn rejected_count(&self) -> usize { self.map.values().filter(|&&v| !v).count() }
}

#[derive(Component)]
pub struct SidebarSpawnBtn;

/// Root container of the module-info panel (name + pool), shown in ModuleMaker only.
#[derive(Component)]
pub struct SidebarModuleInfoRoot;

/// Text node that displays the current module name.
#[derive(Component)]
pub struct SidebarModuleNameText;

/// Text node that displays the current pool name.
#[derive(Component)]
pub struct SidebarPoolNameText;

#[derive(Component)]
pub struct NamingModalRoot;

#[derive(Component)]
pub struct NamingModalText;

#[derive(Resource, Default)]
pub struct SidebarCache {
    pub stems: Vec<String>,
    pub modules: Vec<(String, std::path::PathBuf)>,
    /// Module names in the gallery (same order as gen_index.json), for controller input.
    pub gallery_names: Vec<String>,
}

pub fn spawn_sidebar(commands: &mut Commands, ws: &EditorWorkspace) {
    let width = if ws.sidebar_collapsed {
        Val::Px(34.0)
    } else {
        Val::Px(240.0)
    };
    commands
        .spawn((
            EditorSidebarRoot,
            sidebar_visibility(ws),
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(42.0),
                width,
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(10.0)),
                row_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.92)),
        ))
        .with_children(|root| {
            if !ws.sidebar_collapsed {
                let is_map = ws.workflow == EditorWorkflow::MapMaker;
                let spawn_active = ws.tool == shared::editor_map::EditorTool::SetSpawn;
                root.spawn((
                    SidebarSpawnBtn,
                    Button,
                    Node {
                        display: if is_map { Display::Flex } else { Display::None },
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                        justify_content: JustifyContent::Center,
                        width: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(if spawn_active {
                        Color::srgba(0.18, 0.42, 0.28, 0.98)
                    } else {
                        Color::srgba(0.10, 0.22, 0.16, 0.95)
                    }),
                    Text::new("\u{25B6} Set Spawn"),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(Color::srgb(0.55, 1.0, 0.65)),
                ));
                // Module-info panel: visible in ModuleMaker only.
                root.spawn((
                    SidebarModuleInfoRoot,
                    Node {
                        display: if is_map { Display::None } else { Display::Flex },
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.06, 0.10, 0.16, 0.95)),
                ))
                .with_children(|info| {
                    // Module name row
                    info.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new("Module:"),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(Color::srgb(0.5, 0.58, 0.68)),
                        ));
                        row.spawn((
                            SidebarModuleNameText,
                            Text::new(ws.module.name.clone()),
                            TextFont { font_size: 12.0, ..default() },
                            TextColor(Color::srgb(0.85, 0.95, 1.0)),
                        ));
                    });
                    // Pool row
                    info.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new("Pool:"),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(Color::srgb(0.5, 0.58, 0.68)),
                        ));
                        row.spawn((
                            SidebarPoolNameText,
                            Text::new(ws.place_pool.clone()),
                            TextFont { font_size: 12.0, ..default() },
                            TextColor(Color::srgb(0.85, 0.95, 1.0)),
                        ));
                        row.spawn((
                            SidebarPoolCycle,
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(5.0), Val::Px(3.0)),
                                margin: UiRect::left(Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
                            Text::new("↻"),
                            TextFont { font_size: 12.0, ..default() },
                            TextColor(Color::srgb(0.45, 0.92, 1.0)),
                        ));
                    });
                });

                root.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|tabs| {
                    tabs.spawn((SidebarTabGlb, tab_btn("GLBs", ws.sidebar_tab == SidebarTab::Glb)));
                    tabs.spawn((
                        SidebarTabModule,
                        Node {
                            display: if is_map { Display::Flex } else { Display::None },
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                            flex_grow: 1.0,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Button,
                        BackgroundColor(if ws.sidebar_tab == SidebarTab::Module {
                            Color::srgba(0.14, 0.28, 0.42, 0.98)
                        } else {
                            Color::srgba(0.08, 0.14, 0.22, 0.95)
                        }),
                        Text::new("Modules"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.85, 0.92, 1.0)),
                    ));
                    tabs.spawn((SidebarTabGallery, tab_btn("\u{2605} Pool", ws.sidebar_tab == SidebarTab::Gallery)));
                    tabs.spawn((
                        SidebarTabGenerate,
                        Node {
                            display: if is_map { Display::Flex } else { Display::None },
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                            flex_grow: 1.0,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Button,
                        BackgroundColor(if ws.sidebar_tab == SidebarTab::Generate {
                            Color::srgba(0.14, 0.32, 0.28, 0.98)
                        } else {
                            Color::srgba(0.08, 0.14, 0.22, 0.95)
                        }),
                        Text::new("Proc"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.85, 0.92, 1.0)),
                    ));
                });
                root.spawn((
                    SidebarContent,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        flex_grow: 1.0,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                ))
                .with_children(|scroll| {
                    scroll.spawn((
                        SidebarScrollContent,
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(4.0),
                            position_type: PositionType::Absolute,
                            top: Val::Px(0.0),
                            left: Val::Px(0.0),
                            right: Val::Px(0.0),
                            ..default()
                        },
                    ));
                });
            }
        });
}

fn sidebar_visibility(ws: &EditorWorkspace) -> Visibility {
    Visibility::Inherited
}

fn tab_btn(label: &str, active: bool) -> impl Bundle {
    (
        Button,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            flex_grow: 1.0,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(if active {
            Color::srgba(0.14, 0.28, 0.42, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.22, 0.95)
        }),
        Text::new(label),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
    )
}

fn small_btn(label: &str) -> impl Bundle {
    (
        Button,
        Node {
            padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
        Text::new(label),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.75, 0.85, 0.95)),
    )
}

fn filter_row(group: FilterGroup, on: bool) -> impl Bundle {
    (
        SidebarFilterBtn(group),
        Button,
        Node {
            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.07, 0.12, 0.18, 0.9)),
        Text::new(format!("[{}] {}", if on { "x" } else { " " }, group.label())),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.8, 0.9)),
    )
}

fn spawn_piece_row(parent: &mut ChildSpawnerCommands, idx: usize, stem: &str, selected: bool) {
    let (r, g, b) = editor_catalog::stem_swatch_color(stem);
    parent
        .spawn((
            SidebarPieceBtn(idx),
            Button,
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(if selected {
                Color::srgba(0.18, 0.35, 0.22, 0.98)
            } else {
                Color::srgba(0.08, 0.14, 0.20, 0.92)
            }),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    width: Val::Px(28.0),
                    height: Val::Px(28.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(r, g, b)),
            ));
            row.spawn((
                Text::new(stem),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.82, 0.9, 0.96)),
            ));
        });
}

fn module_row(idx: usize, name: &str, selected: bool) -> impl Bundle {
    (
        SidebarModuleBtn(idx),
        Button,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
            ..default()
        },
        BackgroundColor(if selected {
            Color::srgba(0.22, 0.18, 0.38, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.20, 0.92)
        }),
        Text::new(name),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.88, 0.82, 1.0)),
    )
}

pub fn sync_sidebar_visibility(
    ws: Res<EditorWorkspace>,
    mut q: Query<&mut Visibility, With<EditorSidebarRoot>>,
) {
    let Ok(mut vis) = q.single_mut() else {
        return;
    };
    *vis = sidebar_visibility(&ws);
}

/// Runs every frame to keep button highlights in sync with the current selection
/// without triggering a full sidebar rebuild.
pub fn sync_sidebar_highlight(
    state: Res<EditorState>,
    ws: Res<EditorWorkspace>,
    cache: Res<SidebarCache>,
    mut piece_btns: Query<(&SidebarPieceBtn, &mut BackgroundColor), Without<SidebarModuleBtn>>,
    mut module_btns: Query<(&SidebarModuleBtn, &mut BackgroundColor), Without<SidebarPieceBtn>>,
) {
    for (btn, mut bg) in &mut piece_btns {
        *bg = BackgroundColor(if btn.0 == state.piece_index {
            Color::srgba(0.18, 0.35, 0.22, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.20, 0.92)
        });
    }
    for (btn, mut bg) in &mut module_btns {
        let is_selected = cache
            .modules
            .get(btn.0)
            .map(|(_, p)| Some(p) == ws.selected_module.as_ref())
            .unwrap_or(false);
        *bg = BackgroundColor(if is_selected {
            Color::srgba(0.22, 0.18, 0.38, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.20, 0.92)
        });
    }
}

/// Keeps the module-name and pool-name texts in the sidebar info panel up to date.
pub fn sync_module_info(
    ws: Res<EditorWorkspace>,
    mut name_q: Query<&mut Text, (With<SidebarModuleNameText>, Without<SidebarPoolNameText>)>,
    mut pool_q: Query<&mut Text, (With<SidebarPoolNameText>, Without<SidebarModuleNameText>)>,
) {
    for mut t in &mut name_q {
        let new_val = if ws.module.name.is_empty() {
            "(unnamed)".to_string()
        } else {
            ws.module.name.clone()
        };
        if t.0 != new_val {
            t.0 = new_val;
        }
    }
    for mut t in &mut pool_q {
        if t.0 != ws.place_pool {
            t.0 = ws.place_pool.clone();
        }
    }
}

pub fn rebuild_sidebar(
    mut commands: Commands,
    mut ws: ResMut<EditorWorkspace>,
    mut cache: ResMut<SidebarCache>,
    state: Res<EditorState>,
    ratings: Res<GalleryRatings>,
    map_gen_settings: Res<crate::editor_map_gen::MapGenSettings>,
    map_gen_runtime: Res<crate::editor_map_gen::MapGenRuntime>,
    content: Query<Entity, With<SidebarScrollContent>>,
    mut tab_glb: Query<
        &mut BackgroundColor,
        (With<SidebarTabGlb>, Without<SidebarTabModule>, Without<SidebarTabGallery>, Without<SidebarTabGenerate>),
    >,
    mut tab_mod: Query<
        &mut BackgroundColor,
        (With<SidebarTabModule>, Without<SidebarTabGlb>, Without<SidebarTabGallery>, Without<SidebarTabGenerate>),
    >,
    mut tab_gal: Query<
        &mut BackgroundColor,
        (With<SidebarTabGallery>, Without<SidebarTabGlb>, Without<SidebarTabModule>, Without<SidebarTabGenerate>),
    >,
    mut tab_gen: Query<
        &mut BackgroundColor,
        (With<SidebarTabGenerate>, Without<SidebarTabGlb>, Without<SidebarTabModule>, Without<SidebarTabGallery>),
    >,
    mut mod_tab_node: Query<&mut Node, (With<SidebarTabModule>, Without<SidebarSpawnBtn>)>,
    mut spawn_btn_node: Query<&mut Node, (With<SidebarSpawnBtn>, Without<SidebarTabModule>)>,
    mut module_info_node: Query<
        &mut Node,
        (With<SidebarModuleInfoRoot>, Without<SidebarTabModule>, Without<SidebarSpawnBtn>),
    >,
) {
    if !ws.sidebar_dirty {
        return;
    }
    ws.sidebar_dirty = false;

    // Show/hide workflow-conditional sidebar elements.
    let is_map = ws.workflow == EditorWorkflow::MapMaker;
    if let Ok(mut node) = mod_tab_node.single_mut() {
        node.display = if is_map { Display::Flex } else { Display::None };
    }
    if let Ok(mut node) = spawn_btn_node.single_mut() {
        node.display = if is_map { Display::Flex } else { Display::None };
    }
    if let Ok(mut node) = module_info_node.single_mut() {
        node.display = if is_map { Display::None } else { Display::Flex };
    }

    cache.stems = editor_catalog::filtered_stems(&ws.filters);
    // Safety: if all filters are off the GLB list would be permanently empty.
    // Auto-reset so the editor stays usable no matter what.
    if cache.stems.is_empty() {
        ws.filters.set_all(true);
        cache.stems = editor_catalog::filtered_stems(&ws.filters);
    }
    cache.modules = editor_catalog::list_modules_in_pool(&ws.place_pool);

    // Update tab highlight colours
    if let Ok(mut bg) = tab_glb.single_mut() {
        *bg = BackgroundColor(if ws.sidebar_tab == SidebarTab::Glb {
            Color::srgba(0.14, 0.28, 0.42, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.22, 0.95)
        });
    }
    if let Ok(mut bg) = tab_mod.single_mut() {
        *bg = BackgroundColor(if ws.sidebar_tab == SidebarTab::Module {
            Color::srgba(0.14, 0.28, 0.42, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.22, 0.95)
        });
    }
    if let Ok(mut bg) = tab_gal.single_mut() {
        *bg = BackgroundColor(if ws.sidebar_tab == SidebarTab::Gallery {
            Color::srgba(0.22, 0.14, 0.38, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.22, 0.95)
        });
    }
    if let Ok(mut bg) = tab_gen.single_mut() {
        *bg = BackgroundColor(if ws.sidebar_tab == SidebarTab::Generate {
            Color::srgba(0.14, 0.32, 0.28, 0.98)
        } else {
            Color::srgba(0.08, 0.14, 0.22, 0.95)
        });
    }

    if ws.workflow == EditorWorkflow::ModuleMaker && ws.sidebar_tab == SidebarTab::Module {
        ws.sidebar_tab = SidebarTab::Glb;
    }
    if ws.workflow == EditorWorkflow::ModuleMaker && ws.sidebar_tab == SidebarTab::Generate {
        ws.sidebar_tab = SidebarTab::Glb;
    }

    let Ok(content_ent) = content.single() else {
        return;
    };
    commands.entity(content_ent).despawn_children();

    let gallery_pool = ws.gallery_pool.clone();
    let gallery_cursor = ws.gallery_cursor;
    commands.entity(content_ent).with_children(|parent| {
        match ws.sidebar_tab {
            SidebarTab::Glb => spawn_glb_panel(parent, &ws.filters, &cache.stems, state.piece_index),
            SidebarTab::Module if ws.workflow == EditorWorkflow::MapMaker => {
                spawn_module_panel(parent, &ws.place_pool, &cache.modules, &ws.selected_module)
            }
            SidebarTab::Module => {}
            SidebarTab::Gallery => {
                let entries = editor_catalog::load_gallery_index(&gallery_pool);
                // Store names in cache so gallery_controller_input can use them
                cache.gallery_names = entries.iter().map(|e| e.name.clone()).collect();
                spawn_gallery_panel(parent, &gallery_pool, &entries, &ratings, gallery_cursor);
            }
            SidebarTab::Generate => {
                crate::editor_map_gen::spawn_map_gen_panel(
                    parent,
                    &map_gen_settings,
                    &map_gen_runtime,
                );
            }
        }
    });
}

pub fn select_first_on_tab(ws: &mut EditorWorkspace, state: &mut EditorState, cache: &SidebarCache) {
    match ws.sidebar_tab {
        SidebarTab::Glb => {
            if !cache.stems.is_empty() {
                state.piece_index = 0;
                ws.tool = EditorTool::PlaceGlb;
                ws.respawn_ghost = true;
            }
        }
        SidebarTab::Module => {
            if ws.workflow != EditorWorkflow::MapMaker {
                return;
            }
            if let Some((_, path)) = cache.modules.first() {
                ws.selected_module = Some(path.clone());
                ws.tool = EditorTool::PlaceModule;
                ws.respawn_ghost = true;
            }
        }
        SidebarTab::Gallery => {
            // Gallery is browse-only; no placement tool switch
        }
        SidebarTab::Generate => {
            ws.tool = EditorTool::GalleryPreview;
        }
    }
}

fn spawn_glb_panel(
    parent: &mut ChildSpawnerCommands,
    filters: &PieceFilters,
    stems: &[String],
    selected_index: usize,
) {
    parent.spawn((
        Text::new("Filters"),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.45, 0.92, 1.0)),
    ));
    parent.spawn(Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(4.0),
        ..default()
    }).with_children(|row| {
        row.spawn((SidebarFilterAll, small_btn("All")));
        row.spawn((SidebarFilterNone, small_btn("None")));
    });
    for group in FilterGroup::ALL {
        parent.spawn(filter_row(group, filters.get(group)));
    }
    parent.spawn((
        Text::new(format!("Pieces ({})", stems.len())),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.45, 0.92, 1.0)),
    ));
    for (i, stem) in stems.iter().enumerate() {
        spawn_piece_row(parent, i, stem, i == selected_index);
    }
    if stems.is_empty() {
        parent.spawn((
            Text::new("No GLBs — enable filters above"),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(Color::srgb(0.55, 0.62, 0.72)),
        ));
    }
}

const MODULE_PANEL_LIMIT: usize = 60;

fn spawn_module_panel(
    parent: &mut ChildSpawnerCommands,
    pool: &str,
    modules: &[(String, std::path::PathBuf)],
    selected: &Option<std::path::PathBuf>,
) {
    parent.spawn((
        SidebarPoolCycle,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
        Text::new(format!("Pool: {pool} ({}) ↻", modules.len())),
        TextFont { font_size: 13.0, ..default() },
        TextColor(Color::srgb(0.45, 0.92, 1.0)),
    ));
    if modules.is_empty() {
        parent.spawn((
            Text::new("No saved modules yet.\nUse Module mode + F to save."),
            TextFont { font_size: 12.0, ..default() },
            TextColor(Color::srgb(0.55, 0.6, 0.68)),
        ));
        return;
    }
    let show = modules.len().min(MODULE_PANEL_LIMIT);
    for (i, (name, path)) in modules.iter().take(show).enumerate() {
        let sel = selected.as_ref().is_some_and(|p| p == path);
        parent.spawn(module_row(i, name, sel));
    }
    if modules.len() > MODULE_PANEL_LIMIT {
        parent.spawn((
            Text::new(format!(
                "... {} more — use ★ Gen tab to browse",
                modules.len() - MODULE_PANEL_LIMIT
            )),
            TextFont { font_size: 11.0, ..default() },
            TextColor(Color::srgb(0.45, 0.55, 0.65)),
        ));
    }
}

/// Switch into gallery preview mode: ModuleMaker workflow, neutral tool, load first module.
fn enter_gallery_mode(ws: &mut EditorWorkspace, cache: &SidebarCache) {
    ws.pre_gallery_workflow = Some(ws.workflow);
    ws.sidebar_tab = SidebarTab::Gallery;
    ws.set_workflow(shared::editor_map::EditorWorkflow::ModuleMaker);
    ws.tool = shared::editor_map::EditorTool::GalleryPreview;
    ws.sidebar_dirty = true;
    // Load the focused gallery module so it renders in the viewport immediately.
    if let Some(name) = cache.gallery_names.get(ws.gallery_cursor) {
        let path = shared::editor_map::pool_dir(&ws.gallery_pool)
            .join(format!("{name}.json"));
        ws.load_module_path = Some(path);
    }
}

/// Restore the workflow that was active before gallery mode.
pub fn exit_gallery_mode(ws: &mut EditorWorkspace) {
    if ws.sidebar_tab != SidebarTab::Gallery {
        return;
    }
    let prev = ws.pre_gallery_workflow.take()
        .unwrap_or(shared::editor_map::EditorWorkflow::MapMaker);
    ws.set_workflow(prev);
    ws.tool = shared::editor_map::EditorTool::PlaceGlb;
}

pub fn sidebar_button_input(
    mut ws: ResMut<EditorWorkspace>,
    mut state: ResMut<EditorState>,
    cache: Res<SidebarCache>,
    spawn_btn: Query<&Interaction, (Changed<Interaction>, With<SidebarSpawnBtn>)>,
    glb_tab: Query<&Interaction, (Changed<Interaction>, With<SidebarTabGlb>)>,
    mod_tab: Query<&Interaction, (Changed<Interaction>, With<SidebarTabModule>)>,
    gal_tab: Query<&Interaction, (Changed<Interaction>, With<SidebarTabGallery>)>,
    all_btn: Query<&Interaction, (Changed<Interaction>, With<SidebarFilterAll>)>,
    none_btn: Query<&Interaction, (Changed<Interaction>, With<SidebarFilterNone>)>,
    filt_btn: Query<(&Interaction, &SidebarFilterBtn), Changed<Interaction>>,
    pool_btn: Query<&Interaction, (Changed<Interaction>, With<SidebarPoolCycle>)>,
    piece_btn: Query<(&Interaction, &SidebarPieceBtn), Changed<Interaction>>,
    mod_btn: Query<(&Interaction, &SidebarModuleBtn), Changed<Interaction>>,
) {
    if pressed(&spawn_btn) {
        ws.tool = EditorTool::SetSpawn;
        ws.sidebar_dirty = true;
    }
    if pressed(&glb_tab) {
        exit_gallery_mode(&mut ws);
        ws.sidebar_tab = SidebarTab::Glb;
        ws.sidebar_dirty = true;
        select_first_on_tab(&mut ws, &mut state, &cache);
    }
    if pressed(&mod_tab) {
        exit_gallery_mode(&mut ws);
        ws.sidebar_tab = SidebarTab::Module;
        ws.sidebar_dirty = true;
        select_first_on_tab(&mut ws, &mut state, &cache);
    }
    if pressed(&gal_tab) {
        enter_gallery_mode(&mut ws, &cache);
    }
    if pressed(&all_btn) {
        ws.filters.set_all(true);
        ws.sidebar_dirty = true;
        sync_stems_from_filters(&mut state, &ws.filters);
        ws.respawn_ghost = true;
    }
    if pressed(&none_btn) {
        ws.filters.set_all(false);
        ws.sidebar_dirty = true;
        sync_stems_from_filters(&mut state, &ws.filters);
        ws.respawn_ghost = true;
    }
    for (interaction, btn) in &filt_btn {
        if *interaction == Interaction::Pressed {
            ws.filters.toggle(btn.0);
            ws.sidebar_dirty = true;
            sync_stems_from_filters(&mut state, &ws.filters);
            ws.respawn_ghost = true;
        }
    }
    if pressed(&pool_btn) {
        ws.place_pool = editor_catalog::cycle_pool(&ws.place_pool);
        if ws.workflow == EditorWorkflow::ModuleMaker {
            ws.module.pool = ws.place_pool.clone();
        }
        ws.sidebar_dirty = true;
    }
    for (interaction, btn) in &piece_btn {
        if *interaction == Interaction::Pressed {
            if btn.0 < cache.stems.len() {
                state.piece_index = btn.0;
                ws.tool = EditorTool::PlaceGlb;
                ws.sidebar_dirty = true;
                ws.respawn_ghost = true;
            }
        }
    }
    for (interaction, btn) in &mod_btn {
        if *interaction == Interaction::Pressed {
            if let Some((_, path)) = cache.modules.get(btn.0) {
                if ws.workflow == EditorWorkflow::ModuleMaker {
                    ws.load_module_path = Some(path.clone());
                } else {
                    ws.selected_module = Some(path.clone());
                    ws.tool = EditorTool::PlaceModule;
                    ws.respawn_ghost = true;
                }
            }
        }
    }
}

pub fn sync_stems_from_filters(state: &mut EditorState, filters: &PieceFilters) {
    let stems = editor_catalog::filtered_stems(filters);
    let current = state
        .stems
        .get(state.piece_index)
        .cloned()
        .unwrap_or_default();
    state.stems = stems;
    if state.stems.is_empty() {
        state.piece_index = 0;
        return;
    }
    state.piece_index = state
        .stems
        .iter()
        .position(|s| s == &current)
        .unwrap_or(0)
        .min(state.stems.len() - 1);
}

pub fn sidebar_pointer_and_scroll(
    mut ws: ResMut<EditorWorkspace>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut scroll_content: Query<&mut Node, With<SidebarScrollContent>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        ws.sidebar_pointer_inside = false;
        return;
    };
    let ww = window.width();
    let sidebar_w = if ws.sidebar_collapsed { 34.0 } else { 240.0 };
    ws.sidebar_pointer_inside =
        cursor.x >= ww - sidebar_w && cursor.y >= 42.0 && !ws.sidebar_collapsed;

    if !ws.sidebar_pointer_inside {
        return;
    }
    // Gallery tab uses the wheel for cursor navigation — handled in gallery_controller_input.
    if ws.sidebar_tab == SidebarTab::Gallery {
        wheel.read(); // drain events so they don't pile up
        return;
    }
    let Ok(mut node) = scroll_content.single_mut() else {
        return;
    };
    for ev in wheel.read() {
        let delta = match ev.unit {
            bevy::input::mouse::MouseScrollUnit::Line => ev.y * 24.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => ev.y,
        };
        ws.sidebar_scroll = (ws.sidebar_scroll - delta).max(0.0);
        node.top = Val::Px(-ws.sidebar_scroll);
    }
}

fn pressed(q: &Query<&Interaction, (Changed<Interaction>, impl bevy::ecs::query::QueryFilter + 'static)>) -> bool {
    q.iter().any(|i| *i == Interaction::Pressed)
}

pub fn spawn_naming_modal(commands: &mut Commands, suggested: &str) {
    commands
        .spawn((
            NamingModalRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(20.0)),
                    width: Val::Px(360.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.06, 0.10, 0.16, 0.98)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("New module name"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.45, 0.92, 1.0)),
                ));
                panel.spawn((
                    NamingModalText,
                    Text::new(suggested),
                    TextFont {
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.9, 0.95, 1.0)),
                ));
                panel.spawn((
                    Text::new("Type name · Enter confirm · Esc cancel"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.5, 0.58, 0.68)),
                ));
            });
        });
}

pub fn naming_modal_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut ws: ResMut<EditorWorkspace>,
    modal: Query<Entity, With<NamingModalRoot>>,
    mut text: Query<&mut Text, With<NamingModalText>>,
) {
    let Some(ref mut modal_state) = ws.naming_modal else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        ws.naming_modal = None;
        for e in &modal {
            commands.entity(e).despawn();
        }
        return;
    }
    if keys.just_pressed(KeyCode::Enter) {
        let name = modal_state.buffer.clone();
        let save_as = ws.save_as_requested;
        ws.naming_modal = None;
        ws.save_as_requested = false;
        for e in &modal {
            commands.entity(e).despawn();
        }
        if save_as {
            ws.pending_save_as_name = Some(name);
        } else {
            ws.begin_new_module(&name);
        }
        return;
    }
    if keys.just_pressed(KeyCode::Backspace) {
        modal_state.buffer.pop();
        modal_state.replace_on_first_key = false;
    } else {
        for key in keys.get_just_pressed() {
            if let Some(ch) = key_to_char(*key) {
                if modal_state.replace_on_first_key {
                    modal_state.buffer.clear();
                    modal_state.replace_on_first_key = false;
                }
                modal_state.buffer.push(ch);
            }
        }
    }
    if let Ok(mut t) = text.single_mut() {
        t.0 = modal_state.buffer.clone();
    }
}

// ─── Gallery panel ───────────────────────────────────────────────────────────

fn strategy_short(s: &str) -> &'static str {
    match s {
        "two_rooms"      => "2-room",
        "room_small_corn"=> "corner",
        "room_small_ctr" => "center",
        "corridor_hub"   => "hub",
        "room_wide"      => "wide",
        "room_large"     => "large",
        "free"           => "free",
        _                => "?",
    }
}

fn strategy_color(s: &str) -> Color {
    match s {
        "two_rooms"      => Color::srgb(0.62, 0.45, 0.98),
        "room_small_corn"=> Color::srgb(0.35, 0.88, 0.55),
        "room_small_ctr" => Color::srgb(0.35, 0.85, 0.85),
        "corridor_hub"   => Color::srgb(0.30, 0.95, 0.98),
        "room_wide"      => Color::srgb(0.95, 0.62, 0.28),
        "room_large"     => Color::srgb(0.55, 0.65, 0.85),
        "free"           => Color::srgb(0.98, 0.88, 0.32),
        _                => Color::srgb(0.72, 0.72, 0.72),
    }
}

fn score_badge_color(score: f32) -> Color {
    if score >= 0.92 { Color::srgb(0.22, 0.92, 0.42) }
    else if score >= 0.87 { Color::srgb(0.68, 0.90, 0.22) }
    else if score >= 0.82 { Color::srgb(0.94, 0.82, 0.18) }
    else                   { Color::srgb(0.94, 0.55, 0.18) }
}

fn spawn_gallery_panel(
    parent: &mut ChildSpawnerCommands,
    pool: &str,
    entries: &[shared::editor_catalog::GalleryMeta],
    ratings: &GalleryRatings,
    cursor: usize,
) {
    let pools = shared::editor_catalog::list_gallery_pools();
    if pools.is_empty() {
        parent.spawn((
            Text::new("No generated pools.\nRun:\npython tools/gen_modules.py"),
            TextFont { font_size: 12.0, ..default() },
            TextColor(Color::srgb(0.55, 0.62, 0.72)),
        ));
        return;
    }

    // Pool selector row
    parent.spawn(Node {
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(4.0),
        row_gap: Val::Px(4.0),
        ..default()
    }).with_children(|row| {
        for p in &pools {
            let active = p.as_str() == pool;
            row.spawn((
                GalleryPoolBtn(p.clone()),
                Button,
                Node { padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)), ..default() },
                BackgroundColor(if active {
                    Color::srgba(0.22, 0.14, 0.38, 0.98)
                } else {
                    Color::srgba(0.08, 0.12, 0.20, 0.95)
                }),
                Text::new(p.clone()),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.75, 0.68, 1.0)),
            ));
        }
    });

    let kept = ratings.kept_count();
    let total_rated = ratings.map.len();
    let status = if total_rated == 0 {
        format!("{} modules  |  scroll + LMB/RMB to rate", entries.len())
    } else {
        format!("{} modules  |  {} kept  {} rejected", entries.len(), kept, total_rated - kept)
    };
    parent.spawn((
        Text::new(status),
        TextFont { font_size: 11.0, ..default() },
        TextColor(Color::srgb(0.42, 0.62, 0.72)),
    ));

    // Column header
    parent.spawn(Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(5.0),
        padding: UiRect::axes(Val::Px(5.0), Val::Px(2.0)),
        ..default()
    }).with_children(|hdr| {
        hdr.spawn((Text::new("score"), TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgb(0.4, 0.48, 0.58)),
            Node { width: Val::Px(32.0), ..default() }));
        hdr.spawn((Text::new("name"), TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgb(0.4, 0.48, 0.58)),
            Node { flex_grow: 1.0, ..default() }));
        hdr.spawn((Text::new("E  strat"), TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgb(0.4, 0.48, 0.58))));
        hdr.spawn((Text::new(" ✓ ✗"), TextFont { font_size: 9.0, ..default() },
            TextColor(Color::srgb(0.4, 0.48, 0.58))));
    });

    for (idx, entry) in entries.iter().enumerate() {
        let path = shared::editor_map::pool_dir(pool)
            .join(format!("{}.json", entry.name));
        let rating = ratings.map.get(&entry.name).copied();
        let is_focused = idx == cursor;
        let row_bg = match (rating, is_focused) {
            (Some(true),  true)  => Color::srgba(0.06, 0.28, 0.10, 1.0),   // focused + kept
            (Some(true),  false) => Color::srgba(0.04, 0.16, 0.07, 0.98),  // kept
            (Some(false), true)  => Color::srgba(0.28, 0.06, 0.06, 1.0),   // focused + rejected
            (Some(false), false) => Color::srgba(0.16, 0.04, 0.04, 0.98),  // rejected
            (None,        true)  => Color::srgba(0.14, 0.22, 0.38, 1.0),   // focused (blue)
            (None,        false) => Color::srgba(0.06, 0.10, 0.16, 0.92),  // default
        };

        parent.spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(3.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(row_bg),
        )).with_children(|row| {
            // ── Load button (left portion — score + name + ent + strat) ──────
            row.spawn((
                GalleryEntryBtn(path),
                Button,
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_grow: 1.0,
                    column_gap: Val::Px(4.0),
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            )).with_children(|btn| {
                btn.spawn((
                    Text::new(format!("{:.2}", entry.score)),
                    TextFont { font_size: 10.0, ..default() },
                    TextColor(score_badge_color(entry.score)),
                    Node { width: Val::Px(32.0), ..default() },
                ));
                btn.spawn((
                    Text::new(entry.name.clone()),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(Color::srgb(0.85, 0.92, 1.0)),
                    Node { flex_grow: 1.0, ..default() },
                ));
                btn.spawn((
                    Text::new(format!("{}E", entry.entrances)),
                    TextFont { font_size: 10.0, ..default() },
                    TextColor(Color::srgb(0.48, 0.70, 0.88)),
                ));
                btn.spawn((
                    Text::new(strategy_short(&entry.strategy)),
                    TextFont { font_size: 10.0, ..default() },
                    TextColor(strategy_color(&entry.strategy)),
                ));
            });

            // ── ✓ keep button ──────────────────────────────────────────────
            row.spawn((
                GalleryKeepBtn(entry.name.clone()),
                Button,
                Node { padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)), ..default() },
                BackgroundColor(if rating == Some(true) {
                    Color::srgba(0.12, 0.52, 0.18, 0.98)
                } else {
                    Color::srgba(0.06, 0.18, 0.09, 0.95)
                }),
                Text::new("\u{2713}"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.35, 0.95, 0.45)),
            ));
            // ── ✗ reject button ────────────────────────────────────────────
            row.spawn((
                GalleryRejectBtn(entry.name.clone()),
                Button,
                Node { padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)), margin: UiRect::right(Val::Px(2.0)), ..default() },
                BackgroundColor(if rating == Some(false) {
                    Color::srgba(0.52, 0.10, 0.08, 0.98)
                } else {
                    Color::srgba(0.20, 0.06, 0.06, 0.95)
                }),
                Text::new("\u{2717}"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.95, 0.35, 0.28)),
            ));
        });
    }
}

pub fn gallery_button_input(
    mut ws: ResMut<EditorWorkspace>,
    mut ratings: ResMut<GalleryRatings>,
    entry_btns: Query<(&Interaction, &GalleryEntryBtn), Changed<Interaction>>,
    pool_btns:  Query<(&Interaction, &GalleryPoolBtn),  Changed<Interaction>>,
    keep_btns:  Query<(&Interaction, &GalleryKeepBtn),  Changed<Interaction>>,
    rej_btns:   Query<(&Interaction, &GalleryRejectBtn), Changed<Interaction>>,
) {
    // Load-module click
    for (interaction, btn) in &entry_btns {
        if *interaction == Interaction::Pressed {
            ws.set_workflow(shared::editor_map::EditorWorkflow::ModuleMaker);
            ws.load_module_path = Some(btn.0.clone());
            ws.sidebar_dirty = true;
        }
    }

    // Pool switch
    for (interaction, btn) in &pool_btns {
        if *interaction == Interaction::Pressed {
            ws.gallery_pool = btn.0.clone();
            // Load ratings for new pool
            *ratings = GalleryRatings::load(&btn.0);
            ws.sidebar_dirty = true;
        }
    }

    // Keep toggle
    let mut rating_changed = false;
    for (interaction, btn) in &keep_btns {
        if *interaction == Interaction::Pressed {
            // Ensure ratings are for the current pool
            if ratings.pool != ws.gallery_pool {
                *ratings = GalleryRatings::load(&ws.gallery_pool);
            }
            ratings.toggle_keep(&btn.0);
            rating_changed = true;
        }
    }

    // Reject toggle
    for (interaction, btn) in &rej_btns {
        if *interaction == Interaction::Pressed {
            if ratings.pool != ws.gallery_pool {
                *ratings = GalleryRatings::load(&ws.gallery_pool);
            }
            ratings.toggle_reject(&btn.0);
            rating_changed = true;
        }
    }

    if rating_changed {
        ratings.save();
        ws.sidebar_dirty = true;
    }
}

// ─── Gallery keyboard/mouse controller ───────────────────────────────────────

/// Approximate pixel height of one gallery entry row.
const GALLERY_ROW_H: f32 = 22.0;
/// Approximate pixel height of the gallery header (pool selector + status + column header).
const GALLERY_HEADER_H: f32 = 72.0;

pub fn gallery_controller_input(
    mut ws: ResMut<EditorWorkspace>,
    mut ratings: ResMut<GalleryRatings>,
    cache: Res<SidebarCache>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mouse: Res<ButtonInput<bevy::input::mouse::MouseButton>>,
    mut scroll_content: Query<&mut Node, With<SidebarScrollContent>>,
) {
    // Only active when Gallery tab is shown AND mouse is over the work area (not the sidebar).
    if ws.sidebar_tab != SidebarTab::Gallery || ws.sidebar_pointer_inside {
        return;
    }
    let n = cache.gallery_names.len();
    if n == 0 {
        return;
    }

    // ── mousewheel → cycle module ─────────────────────────────────────────
    let mut cursor_delta: i32 = 0;
    for ev in wheel.read() {
        let lines = match ev.unit {
            bevy::input::mouse::MouseScrollUnit::Line  => ev.y as i32,
            bevy::input::mouse::MouseScrollUnit::Pixel => (ev.y / 24.0) as i32,
        };
        cursor_delta -= lines; // scroll down = next module
    }
    if cursor_delta != 0 {
        ws.gallery_cursor = ((ws.gallery_cursor as i64 + cursor_delta as i64)
            .rem_euclid(n as i64)) as usize;
        ws.sidebar_dirty = true;

        // Load the new module into the viewport
        if let Some(name) = cache.gallery_names.get(ws.gallery_cursor) {
            let path = shared::editor_map::pool_dir(&ws.gallery_pool)
                .join(format!("{name}.json"));
            ws.load_module_path = Some(path);
        }

        // Scroll the sidebar list to keep the focused row visible
        let row_top = GALLERY_HEADER_H + ws.gallery_cursor as f32 * GALLERY_ROW_H;
        let target_scroll = (row_top - 160.0).max(0.0);
        ws.sidebar_scroll = target_scroll;
        if let Ok(mut node) = scroll_content.single_mut() {
            node.top = Val::Px(-target_scroll);
        }
    }

    // ── LMB → keep, RMB → reject ─────────────────────────────────────────
    let vote_keep   = mouse.just_pressed(bevy::input::mouse::MouseButton::Left);
    let vote_reject = mouse.just_pressed(bevy::input::mouse::MouseButton::Right);
    if vote_keep || vote_reject {
        if let Some(name) = cache.gallery_names.get(ws.gallery_cursor) {
            if ratings.pool != ws.gallery_pool {
                *ratings = GalleryRatings::load(&ws.gallery_pool);
            }
            if vote_keep   { ratings.toggle_keep(name); }
            if vote_reject { ratings.toggle_reject(name); }
            ratings.save();
            ws.sidebar_dirty = true;
        }
    }
}

// ─── Load file picker ────────────────────────────────────────────────────────

#[derive(Component)]
pub struct LoadPickerRoot;

/// Each entry button in the load picker carries the path it will load.
#[derive(Component)]
pub struct LoadPickerEntry(pub std::path::PathBuf);

#[derive(Component)]
pub struct LoadPickerCancelBtn;

pub fn spawn_load_picker_ui(mut commands: Commands, mut ws: ResMut<EditorWorkspace>) {
    if !ws.pending_load_picker {
        return;
    }
    ws.pending_load_picker = false;

    let entries: Vec<(String, std::path::PathBuf)> = match ws.workflow {
        EditorWorkflow::MapMaker => list_map_files(),
        EditorWorkflow::ModuleMaker => editor_catalog::list_modules_in_pool(&ws.place_pool),
    };

    let title = match ws.workflow {
        EditorWorkflow::MapMaker => "Load map",
        EditorWorkflow::ModuleMaker => "Load module",
    };

    commands
        .spawn((
            LoadPickerRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.60)),
            ZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(20.0)),
                    width: Val::Px(380.0),
                    max_height: Val::Vh(70.0),
                    overflow: Overflow::clip_y(),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.09, 0.14, 0.98)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(title),
                    TextFont { font_size: 16.0, ..default() },
                    TextColor(Color::srgb(0.45, 0.92, 1.0)),
                ));
                if entries.is_empty() {
                    panel.spawn((
                        Text::new("No files found."),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.55, 0.6, 0.68)),
                    ));
                }
                for (name, path) in entries {
                    panel
                        .spawn((
                            LoadPickerEntry(path),
                            Button,
                            Node {
                                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Text::new(name),
                                TextFont { font_size: 13.0, ..default() },
                                TextColor(Color::srgb(0.85, 0.92, 1.0)),
                            ));
                        });
                }
                panel.spawn((
                    LoadPickerCancelBtn,
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        margin: UiRect::top(Val::Px(6.0)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.22, 0.10, 0.10, 0.95)),
                    Text::new("Cancel"),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.65, 0.65)),
                ));
            });
        });
}

pub fn load_picker_input(
    mut commands: Commands,
    mut ws: ResMut<EditorWorkspace>,
    entry_btns: Query<(&Interaction, &LoadPickerEntry), Changed<Interaction>>,
    cancel_btn: Query<&Interaction, (Changed<Interaction>, With<LoadPickerCancelBtn>)>,
    roots: Query<Entity, With<LoadPickerRoot>>,
) {
    let mut close = false;

    for (interaction, entry) in &entry_btns {
        if *interaction == Interaction::Pressed {
            match ws.workflow {
                EditorWorkflow::MapMaker => {
                    ws.pending_load_map = Some(entry.0.clone());
                }
                EditorWorkflow::ModuleMaker => {
                    ws.load_module_path = Some(entry.0.clone());
                }
            }
            close = true;
        }
    }

    for interaction in &cancel_btn {
        if *interaction == Interaction::Pressed {
            close = true;
        }
    }

    if close {
        for e in &roots {
            commands.entity(e).despawn();
        }
    }
}

fn list_map_files() -> Vec<(String, std::path::PathBuf)> {
    let dir = shared::editor_map::maps_dir();
    let mut maps = Vec::new();
    if let Ok(read) = std::fs::read_dir(&dir) {
        for ent in read.flatten() {
            let path = ent.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_string();
                maps.push((name, path));
            }
        }
    }
    maps.sort_by(|a, b| a.0.cmp(&b.0));
    maps
}

fn key_to_char(key: KeyCode) -> Option<char> {
    match key {
        KeyCode::KeyA => Some('a'),
        KeyCode::KeyB => Some('b'),
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyD => Some('d'),
        KeyCode::KeyE => Some('e'),
        KeyCode::KeyF => Some('f'),
        KeyCode::KeyG => Some('g'),
        KeyCode::KeyH => Some('h'),
        KeyCode::KeyI => Some('i'),
        KeyCode::KeyJ => Some('j'),
        KeyCode::KeyK => Some('k'),
        KeyCode::KeyL => Some('l'),
        KeyCode::KeyM => Some('m'),
        KeyCode::KeyN => Some('n'),
        KeyCode::KeyO => Some('o'),
        KeyCode::KeyP => Some('p'),
        KeyCode::KeyQ => Some('q'),
        KeyCode::KeyR => Some('r'),
        KeyCode::KeyS => Some('s'),
        KeyCode::KeyT => Some('t'),
        KeyCode::KeyU => Some('u'),
        KeyCode::KeyV => Some('v'),
        KeyCode::KeyW => Some('w'),
        KeyCode::KeyX => Some('x'),
        KeyCode::KeyY => Some('y'),
        KeyCode::KeyZ => Some('z'),
        KeyCode::Digit0 => Some('0'),
        KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'),
        KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'),
        KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'),
        KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'),
        KeyCode::Digit9 => Some('9'),
        KeyCode::Minus => Some('-'),
        _ => None,
    }
}
