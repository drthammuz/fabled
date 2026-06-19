//! Window display mode for editor / playtest (persisted in `editor_settings.json`).

use bevy::prelude::*;
use bevy::window::{MonitorSelection, VideoModeSelection, WindowMode};
use shared::editor_settings::DisplayMode;
use shared::EditorMode;
use shared::TestMode;

pub fn window_mode_for(display: DisplayMode) -> WindowMode {
    match display {
        DisplayMode::Windowed => WindowMode::Windowed,
        DisplayMode::BorderlessFullscreen => {
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        }
        DisplayMode::Fullscreen => {
            WindowMode::Fullscreen(MonitorSelection::Primary, VideoModeSelection::Current)
        }
    }
}

pub struct DisplaySettingsPlugin;

impl Plugin for DisplaySettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_window_mode);
    }
}

/// Re-apply when settings change in the editor panel.
fn sync_window_mode(
    editor: Option<Res<EditorMode>>,
    test: Option<Res<TestMode>>,
    settings: Res<shared::editor_settings::UserEditorPrefs>,
    mut window: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    if editor.is_none() && test.is_none() {
        return;
    }
    let target = if editor.is_some() {
        settings.editor_display
    } else {
        settings.test_display
    };
    let Ok(mut window) = window.single_mut() else {
        return;
    };
    let mode = window_mode_for(target);
    if window.mode != mode {
        window.mode = mode;
    }
}
