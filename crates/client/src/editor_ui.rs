//! Editor toolbar, dropdown menus, and chrome.

use bevy::prelude::*;
use shared::editor_map::{EditorTool, EditorWorkflow};
use shared::editor_settings::UserEditorPrefs;

use crate::editor_sidebar::spawn_sidebar;
use crate::editor_state::EditorState;
use crate::editor_workspace::{EditorMenuRoot, EditorToolbarRoot, EditorWorkspace};

#[derive(Component, Clone, Copy)]
pub enum ToolbarBtn {
    ModeToggle,
    Select,
    Place,
    Snap,
}

#[derive(Component, Clone, Copy)]
pub struct MenuDropdownPanel;

#[derive(Component, Clone, Copy)]
pub enum MenuBarBtn {
    File,
    Options,
    Actions,
}

#[derive(Component, Clone, Copy)]
pub enum FileAction {
    New,
    Discard,
    Save,
    SaveAs,
    Load,
}

#[derive(Component, Clone, Copy)]
pub enum OptionsAction {
    MapSizeX,
    MapSizeZ,
    EditorDisplay,
    TestDisplay,
}

#[derive(Component, Clone, Copy)]
pub enum ActionsAction {
    AddFloor,
    RemoveFloor,
}

#[derive(Component, Clone, Copy)]
pub enum MenuLabel {
    Status,
    Snap,
    ModeToggle,
}

pub fn spawn_editor_chrome(
    commands: &mut Commands,
    ws: &EditorWorkspace,
    state: &EditorState,
    _prefs: &UserEditorPrefs,
) {
    spawn_toolbar(commands, ws, state);
    spawn_sidebar(commands, ws);
}

fn spawn_toolbar(commands: &mut Commands, ws: &EditorWorkspace, state: &EditorState) {
    commands
        .spawn((
            EditorToolbarRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.92)),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                ..default()
            })
            .with_children(|bar| {
                bar.spawn(menu_header_btn("File", MenuBarBtn::File));
                bar.spawn(menu_header_btn("Options", MenuBarBtn::Options));
                bar.spawn(menu_header_btn("Actions", MenuBarBtn::Actions));
                bar.spawn((
                    MenuLabel::Status,
                    Text::new(status_line(ws, state)),
                    TextFont {
                        font_size: 12.5,
                        ..default()
                    },
                    TextColor(Color::srgb(0.55, 0.65, 0.75)),
                    Node {
                        margin: UiRect::left(Val::Px(12.0)),
                        flex_grow: 1.0,
                        ..default()
                    },
                ));
                if !ws.dressing_only {
                    bar.spawn(toolbar_btn_label(
                        ToolbarBtn::ModeToggle,
                        MenuLabel::ModeToggle,
                        mode_btn_text(ws),
                    ));
                } else {
                    bar.spawn((
                        Text::new("Synth vignette"),
                        TextFont {
                            font_size: 12.5,
                            ..default()
                        },
                        TextColor(Color::srgb(0.55, 0.75, 0.95)),
                        Node {
                            margin: UiRect::left(Val::Px(8.0)),
                            ..default()
                        },
                    ));
                }
                bar.spawn(toolbar_btn("Select", ToolbarBtn::Select));
                bar.spawn(toolbar_btn("Place", ToolbarBtn::Place));
                bar.spawn(toolbar_btn_label(
                    ToolbarBtn::Snap,
                    MenuLabel::Snap,
                    format!("Snap: {}", ws.snap.label()),
                ));
            });

            root.spawn((
                EditorMenuRoot,
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(0.0)),
                    min_height: Val::Px(0.0),
                    ..default()
                },
            ));
        });
}

fn mode_btn_text(ws: &EditorWorkspace) -> String {
    match ws.workflow {
        EditorWorkflow::MapMaker => "Mode: Map".into(),
        EditorWorkflow::ModuleMaker => "Mode: Module".into(),
        EditorWorkflow::SynthDressing => "Mode: Dressing".into(),
    }
}

fn menu_header_btn(label: &str, kind: MenuBarBtn) -> impl Bundle {
    (
        kind,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
        Text::new(label),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
    )
}

fn toolbar_btn(label: &str, action: ToolbarBtn) -> impl Bundle {
    (
        action,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
        Text::new(label),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
    )
}

fn toolbar_btn_label(action: ToolbarBtn, label_kind: MenuLabel, text: String) -> impl Bundle {
    (
        action,
        label_kind,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
        Text::new(text),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
    )
}

fn dropdown_btn(label: String, action: impl Bundle) -> impl Bundle {
    (
        action,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
            width: Val::Px(140.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.08, 0.14, 0.22, 0.98)),
        Text::new(label),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.82, 0.9, 0.98)),
    )
}

pub fn status_line(ws: &EditorWorkspace, state: &EditorState) -> String {
    let grid = ws.grid();
    if ws.dressing_only || ws.workflow == shared::editor_map::EditorWorkflow::SynthDressing {
        let elev = state.dressing_y_steps;
        let elev_label = if elev == 0 {
            String::new()
        } else {
            format!(" · elev {:+} ({:+.1} m)", elev, elev as f32 * shared::editor_catalog::SYNTH_DECK_Y)
        };
        return format!(
            "Dressing · {} · {}×{} · mouse4/5 rotate · F face hover{}",
            ws.tool.label(),
            grid.cells_x,
            grid.cells_z,
            elev_label,
        );
    }
    format!(
        "{} · {} · floor {} · {}×{} cells",
        ws.workflow.label(),
        ws.tool.label(),
        ws.floor_level,
        grid.cells_x,
        grid.cells_z,
    )
}

pub fn sync_dropdown_menus(
    mut commands: Commands,
    ws: Res<EditorWorkspace>,
    prefs: Res<UserEditorPrefs>,
    menu_root: Query<Entity, With<EditorMenuRoot>>,
    children: Query<&Children>,
    mut last: Local<(bool, bool, bool, EditorWorkflow, u32, u32)>,
) {
    let state = (
        ws.file_menu_open,
        ws.options_menu_open,
        ws.actions_menu_open,
        ws.workflow,
        ws.map.modules_x,
        ws.map.modules_z,
    );
    if *last == state {
        return;
    }
    *last = state;

    let Ok(root) = menu_root.single() else {
        return;
    };
    for child in children.iter_descendants(root) {
        commands.entity(child).despawn();
    }

    if !ws.file_menu_open && !ws.options_menu_open && !ws.actions_menu_open {
        return;
    }

    commands.entity(root).with_children(|parent| {
        if ws.file_menu_open {
            parent
                .spawn((
                    MenuDropdownPanel,
                    Button,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.96)),
                ))
                .with_children(|col| {
                    if ws.workflow == EditorWorkflow::ModuleMaker {
                        col.spawn(dropdown_btn("New".into(), FileAction::New));
                    } else if ws.workflow == EditorWorkflow::SynthDressing {
                        col.spawn(dropdown_btn("New vignette".into(), FileAction::New));
                    } else {
                        col.spawn(dropdown_btn("New map".into(), FileAction::New));
                    }
                    col.spawn(dropdown_btn("Discard".into(), FileAction::Discard));
                    col.spawn(dropdown_btn("Save".into(), FileAction::Save));
                    col.spawn(dropdown_btn("Save as…".into(), FileAction::SaveAs));
                    col.spawn(dropdown_btn("Load".into(), FileAction::Load));
                });
        }
        if ws.options_menu_open {
            parent
                .spawn((
                    MenuDropdownPanel,
                    Button,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.96)),
                ))
                .with_children(|col| {
                    if ws.workflow == EditorWorkflow::MapMaker {
                        col.spawn(dropdown_btn(
                            format!("Map width: {} mod", ws.map.modules_x),
                            OptionsAction::MapSizeX,
                        ));
                        col.spawn(dropdown_btn(
                            format!("Map height: {} mod", ws.map.modules_z),
                            OptionsAction::MapSizeZ,
                        ));
                    }
                    col.spawn(dropdown_btn(
                        format!("Editor display: {}", prefs.editor_display.label()),
                        OptionsAction::EditorDisplay,
                    ));
                    col.spawn(dropdown_btn(
                        format!("Playtest display: {}", prefs.test_display.label()),
                        OptionsAction::TestDisplay,
                    ));
                });
        }
        if ws.actions_menu_open {
            parent
                .spawn((
                    MenuDropdownPanel,
                    Button,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        padding: UiRect::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.04, 0.07, 0.11, 0.96)),
                ))
                .with_children(|col| {
                    col.spawn(dropdown_btn("Add floor".into(), ActionsAction::AddFloor));
                    col.spawn(dropdown_btn("Remove floor".into(), ActionsAction::RemoveFloor));
                });
        }
    });
}

pub fn menu_button_input(
    mut ws: ResMut<EditorWorkspace>,
    mut prefs: ResMut<UserEditorPrefs>,
    headers: Query<(&Interaction, &MenuBarBtn), Changed<Interaction>>,
    toolbar: Query<(&Interaction, &ToolbarBtn), (Changed<Interaction>, Without<MenuBarBtn>)>,
    file_btns: Query<(&Interaction, &FileAction), (Changed<Interaction>, Without<MenuBarBtn>)>,
    opt_btns: Query<(&Interaction, &OptionsAction), (Changed<Interaction>, Without<MenuBarBtn>)>,
    act_btns: Query<(&Interaction, &ActionsAction), (Changed<Interaction>, Without<MenuBarBtn>)>,
) {
    for (interaction, kind) in &headers {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match kind {
            MenuBarBtn::File => {
                ws.file_menu_open = !ws.file_menu_open;
                ws.options_menu_open = false;
                ws.actions_menu_open = false;
            }
            MenuBarBtn::Options => {
                ws.options_menu_open = !ws.options_menu_open;
                ws.file_menu_open = false;
                ws.actions_menu_open = false;
            }
            MenuBarBtn::Actions => {
                ws.actions_menu_open = !ws.actions_menu_open;
                ws.file_menu_open = false;
                ws.options_menu_open = false;
            }
        }
    }

    for (interaction, btn) in &toolbar {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match btn {
            ToolbarBtn::ModeToggle => ws.toggle_workflow(),
            ToolbarBtn::Select => ws.tool = EditorTool::Select,
            ToolbarBtn::Place => {
                ws.tool = if ws.workflow == EditorWorkflow::MapMaker
                    && ws.sidebar_tab == crate::editor_sidebar::SidebarTab::Module
                {
                    EditorTool::PlaceModule
                } else {
                    EditorTool::PlaceGlb
                };
            }
            ToolbarBtn::Snap => ws.snap = ws.snap.next(),
        }
        ws.close_menus();
    }

    for (interaction, action) in &file_btns {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            FileAction::New => {
                if ws.workflow == EditorWorkflow::ModuleMaker {
                    ws.open_naming_modal();
                } else {
                    ws.file_new = true;
                }
            }
            FileAction::Discard => ws.file_discard = true,
            FileAction::Save => ws.file_save = true,
            FileAction::SaveAs => ws.file_save_as = true,
            FileAction::Load => ws.file_load = true,
        }
        ws.close_menus();
    }

    for (interaction, action) in &opt_btns {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            OptionsAction::MapSizeX => ws.cycle_map_modules_x(),
            OptionsAction::MapSizeZ => ws.cycle_map_modules_z(),
            OptionsAction::EditorDisplay => {
                prefs.editor_display = prefs.editor_display.next();
                let _ = prefs.save();
            }
            OptionsAction::TestDisplay => {
                prefs.test_display = prefs.test_display.next();
                let _ = prefs.save();
            }
        }
        ws.close_menus();
    }

    for (interaction, action) in &act_btns {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            ActionsAction::AddFloor => {
                ws.tool_before_floor = ws.tool;
                ws.tool = EditorTool::FloorAdd;
            }
            ActionsAction::RemoveFloor => {
                ws.tool_before_floor = ws.tool;
                ws.tool = EditorTool::FloorRemove;
            }
        }
        ws.close_menus();
    }
}

pub fn sync_menu_labels(
    ws: Res<EditorWorkspace>,
    state: Res<crate::editor_state::EditorState>,
    mut labels: Query<(&MenuLabel, &mut Text)>,
) {
    for (kind, mut text) in &mut labels {
        text.0 = match kind {
            MenuLabel::Status => status_line(&ws, &state),
            MenuLabel::Snap => format!("Snap: {}", ws.snap.label()),
            MenuLabel::ModeToggle => mode_btn_text(&ws),
        };
    }
}

/// Updates `ws.pointer_over_ui` each frame so `editor_input` can skip 3-D placement when the
/// cursor is over any interactive UI button (toolbar, dropdown items, sidebar buttons, etc.).
pub fn update_ui_hover_block(
    mut ws: ResMut<EditorWorkspace>,
    any_btn: Query<&Interaction, With<Button>>,
) {
    ws.pointer_over_ui = any_btn
        .iter()
        .any(|i| matches!(*i, Interaction::Hovered | Interaction::Pressed));
}

pub fn cancel_floor_tool(keys: Res<ButtonInput<KeyCode>>, mut ws: ResMut<EditorWorkspace>) {
    if keys.just_pressed(KeyCode::Escape)
        && matches!(ws.tool, EditorTool::FloorAdd | EditorTool::FloorRemove)
    {
        ws.tool = ws.tool_before_floor;
        ws.floor_painting = false;
        ws.floor_paint_preview = None;
        ws.floor_dirty = true;
    }
}

/// Close File / Options / Actions dropdowns when the pointer leaves the menu bar zone.
/// We require several consecutive frames with nothing hovered to tolerate the single-frame
/// gap while the cursor moves from a header button to the dropdown panel below it.
pub fn close_menus_on_pointer_leave(
    mut ws: ResMut<EditorWorkspace>,
    headers: Query<&Interaction, With<MenuBarBtn>>,
    dropdowns: Query<&Interaction, With<MenuDropdownPanel>>,
    file_btns: Query<&Interaction, With<FileAction>>,
    opt_btns: Query<&Interaction, With<OptionsAction>>,
    act_btns: Query<&Interaction, With<ActionsAction>>,
    mut frames_out: Local<u8>,
) {
    if !ws.file_menu_open && !ws.options_menu_open && !ws.actions_menu_open {
        *frames_out = 0;
        return;
    }
    let hovered = |i: &Interaction| matches!(*i, Interaction::Hovered | Interaction::Pressed);
    let any_hovered = headers.iter().any(hovered)
        || dropdowns.iter().any(hovered)
        || file_btns.iter().any(hovered)
        || opt_btns.iter().any(hovered)
        || act_btns.iter().any(hovered);
    if any_hovered {
        *frames_out = 0;
    } else {
        *frames_out += 1;
        if *frames_out >= 4 {
            *frames_out = 0;
            ws.close_menus();
        }
    }
}
