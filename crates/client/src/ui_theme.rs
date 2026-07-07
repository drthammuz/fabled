//! Shared UI style for the gameplay HUD and overlays (Pass C framework —
//! Pass D dialogue/trade windows reuse these builders). One palette, one
//! panel recipe, so every window reads as the same product.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

// --- Font ---------------------------------------------------------------

/// The one font for every window-UI text (Cascadia Mono, OFL — bundled in
/// assets/fonts). Loaded once; `apply_ui_font` stamps it onto every text
/// entity that still carries the engine-default font handle.
///
/// Why not just use the default font: the 2026-07-07 playtest showed single
/// glyphs inside dialogue strings rendering at roughly half size (market2.png)
/// — a glyph-atlas artifact in the engine-default font's shared atlas, which
/// terminal Text2d (offscreen render target), the editor UI and the gameplay
/// HUD all wrote into. Giving window UI its own font asset gives it its own
/// atlas namespace; the terminal keeps the default Fira Mono (its 54-col
/// shell grid is tuned to those metrics) on a render layer we skip here.
#[derive(Resource)]
pub struct UiFont(pub Handle<Font>);

pub struct UiThemePlugin;

impl Plugin for UiThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_ui_font)
            .add_systems(PostUpdate, apply_ui_font);
    }
}

fn load_ui_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(UiFont(asset_server.load("fonts/CascadiaMono.ttf")));
}

fn apply_ui_font(
    font: Res<UiFont>,
    mut texts: Query<(&mut TextFont, Option<&RenderLayers>), Added<TextFont>>,
) {
    for (mut tf, layers) in &mut texts {
        // Offscreen-layer text (the in-world terminal) keeps its own font.
        if layers.is_some() {
            continue;
        }
        if tf.font == Handle::default() {
            tf.font = font.0.clone();
        }
    }
}

// --- Palette -----------------------------------------------------------

/// Panel background — near-black blue, translucent.
pub const PANEL_BG: Color = Color::srgba(0.03, 0.05, 0.08, 0.82);
/// Panel border — faint steel blue.
pub const PANEL_BORDER: Color = Color::srgba(0.35, 0.55, 0.75, 0.35);
/// Primary text.
pub const TEXT: Color = Color::srgb(0.88, 0.92, 0.95);
/// Secondary / label text.
pub const TEXT_DIM: Color = Color::srgba(0.55, 0.65, 0.72, 0.9);
/// Accent — cyan, used for selection and headings.
pub const ACCENT: Color = Color::srgb(0.35, 0.85, 0.95);
/// Danger — damage, death, low HP.
pub const DANGER: Color = Color::srgb(0.92, 0.25, 0.22);
/// Warning — mid HP, credits.
pub const WARN: Color = Color::srgb(0.95, 0.75, 0.25);
/// Good — full HP, extraction.
pub const GOOD: Color = Color::srgb(0.35, 0.85, 0.45);

// --- Builders -----------------------------------------------------------

/// Standard bordered panel node. Callers add position/size/layout on top.
pub fn panel() -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            border: UiRect::all(Val::Px(1.0)),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
    )
}

/// Heading text bundle (small caps feel via size + accent color).
pub fn heading(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont { font_size: 12.0, ..default() },
        TextColor(ACCENT),
    )
}

/// Body text bundle.
pub fn body(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont { font_size: 14.0, ..default() },
        TextColor(TEXT),
    )
}

/// Dim label text bundle.
pub fn label(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont { font_size: 11.0, ..default() },
        TextColor(TEXT_DIM),
    )
}

// --- Item styling -------------------------------------------------------

/// Short tag + accent color per item id, used as the v1 "icon" in the hotbar
/// and inventory grid (no icon art assets yet — a colored chip reads faster
/// than a name and stays honest until real icons land).
pub fn item_style(id: u32) -> (&'static str, Color) {
    match id {
        shared::items::FLASHLIGHT => ("FLA", Color::srgb(0.95, 0.85, 0.45)),
        shared::items::MAP => ("MAP", Color::srgb(0.45, 0.85, 0.95)),
        shared::items::PIPE_BAT => ("BAT", Color::srgb(0.80, 0.55, 0.45)),
        shared::items::CREDITS => ("CRD", Color::srgb(0.95, 0.75, 0.25)),
        shared::items::SCRAP => ("SCR", Color::srgb(0.75, 0.55, 0.35)),
        shared::items::MEDICAL_BAG => ("MED", Color::srgb(0.40, 0.90, 0.55)),
        shared::items::HACKER_DEVICE => ("HAK", Color::srgb(0.75, 0.55, 0.95)),
        shared::items::SCRAP_PISTOL => ("GUN", Color::srgb(0.60, 0.75, 0.90)),
        _ => ("???", Color::srgb(0.6, 0.6, 0.6)),
    }
}
