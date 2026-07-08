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

/// Panel background — deep blue-black, mostly opaque so text stays crisp.
pub const PANEL_BG: Color = Color::srgba(0.04, 0.06, 0.10, 0.92);
/// Slightly lighter inset surface (rows, slots, wells) for layered depth.
pub const SURFACE: Color = Color::srgba(0.10, 0.14, 0.20, 0.85);
/// Panel border — steel blue, brighter than before so panels read as framed.
pub const PANEL_BORDER: Color = Color::srgba(0.40, 0.62, 0.82, 0.55);
/// Primary text.
pub const TEXT: Color = Color::srgb(0.90, 0.94, 0.97);
/// Secondary / label text.
pub const TEXT_DIM: Color = Color::srgba(0.58, 0.68, 0.76, 0.9);
/// Accent — cyan, used for selection and headings.
pub const ACCENT: Color = Color::srgb(0.35, 0.88, 0.98);
/// Dim accent — inactive selections / hint borders.
pub const ACCENT_DIM: Color = Color::srgba(0.30, 0.62, 0.72, 0.55);
/// Danger — damage, death, low HP.
pub const DANGER: Color = Color::srgb(0.95, 0.28, 0.24);
/// Warning — mid HP, credits.
pub const WARN: Color = Color::srgb(0.98, 0.78, 0.28);
/// Good — full HP, extraction.
pub const GOOD: Color = Color::srgb(0.40, 0.90, 0.50);

/// Corner rounding used across every window/panel/slot for a consistent feel.
pub const RADIUS: Val = Val::Px(10.0);
/// Tighter rounding for small controls (chips, rows, hotbar slots).
pub const RADIUS_SM: Val = Val::Px(6.0);

// --- Builders -----------------------------------------------------------

/// Standard bordered panel node. Callers add position/size/layout on top.
pub fn panel() -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            border: UiRect::all(Val::Px(1.5)),
            padding: UiRect::all(Val::Px(10.0)),
            border_radius: BorderRadius::all(RADIUS),
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
    )
}

/// Inset surface node (a well/row inside a panel) — rounded, subtle fill.
pub fn surface() -> (Node, BackgroundColor) {
    (
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
            border_radius: BorderRadius::all(RADIUS_SM),
            ..default()
        },
        BackgroundColor(SURFACE),
    )
}

/// Heading text bundle (small caps feel via size + accent color).
pub fn heading(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont { font_size: 15.0, ..default() },
        TextColor(ACCENT),
    )
}

/// Body text bundle.
pub fn body(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont { font_size: 15.0, ..default() },
        TextColor(TEXT),
    )
}

/// Dim label text bundle.
pub fn label(text: &str) -> (Text, TextFont, TextColor) {
    (
        Text::new(text),
        TextFont { font_size: 12.0, ..default() },
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
        shared::items::BANDAGE => ("BND", Color::srgb(0.90, 0.90, 0.85)),
        shared::items::TAGGER => ("TAG", Color::srgb(0.45, 0.95, 0.80)),
        shared::items::ARMOR => ("ARM", Color::srgb(0.70, 0.72, 0.78)),
        shared::items::DATA_SHARD => ("DAT", Color::srgb(0.55, 0.80, 1.0)),
        _ => ("???", Color::srgb(0.6, 0.6, 0.6)),
    }
}
