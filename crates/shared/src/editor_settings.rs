//! Persisted editor / playtest UI preferences.

use serde::{Deserialize, Serialize};

pub const SETTINGS_PATH: &str = "userinput/editor_settings.json";

fn settings_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(SETTINGS_PATH)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    #[default]
    Windowed,
    BorderlessFullscreen,
    Fullscreen,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 3] = [
        DisplayMode::Windowed,
        DisplayMode::BorderlessFullscreen,
        DisplayMode::Fullscreen,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DisplayMode::Windowed => "Windowed",
            DisplayMode::BorderlessFullscreen => "Borderless",
            DisplayMode::Fullscreen => "Fullscreen",
        }
    }

    pub fn next(self) -> Self {
        match self {
            DisplayMode::Windowed => DisplayMode::BorderlessFullscreen,
            DisplayMode::BorderlessFullscreen => DisplayMode::Fullscreen,
            DisplayMode::Fullscreen => DisplayMode::Windowed,
        }
    }
}

#[derive(bevy::prelude::Resource, Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserEditorPrefs {
    #[serde(default)]
    pub editor_display: DisplayMode,
    #[serde(default)]
    pub test_display: DisplayMode,
}

impl UserEditorPrefs {
    pub fn load() -> Self {
        let path = settings_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }
}
