//! Live procedural map generation panel (sidebar **Proc** tab).
//!
//! Spawns `tools/gen_maps.py --preview` in a background thread and reloads the
//! preview map into the editor when complete.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use bevy::prelude::*;
use shared::editor_map::MAP_GEN_PREVIEW_PATH;

use crate::editor_sidebar::SidebarTab;
use crate::editor_workspace::EditorWorkspace;

const DEBOUNCE_SECS: f32 = 0.75;

const FACTION_PROFILES: [&str; 5] = [
    "industrial_default",
    "priesthood",
    "synth",
    "outlaw",
    "necropolis",
];

#[derive(Resource, Clone, Debug)]
pub struct MapGenSettings {
    pub seed: u32,
    pub attempts: u32,
    // Real free-form generator knobs (forwarded to tools/gen_maps.py --preview).
    pub cells: u32,
    pub rooms: u32,
    pub loops: u32,
    pub organicness: f32,
    pub corridor_width: f32,
    pub hidden: f32,
    /// `single` = one profile; `transition` = start / middle / end zones.
    pub mix_mode: String,
    pub faction_profile: String,
    pub prev_faction: String,
    pub next_faction: String,
    pub default_faction: String,
    pub prev_fraction: f32,
    pub default_fraction: f32,
    pub next_fraction: f32,
    pub auto_regen: bool,
}

impl Default for MapGenSettings {
    fn default() -> Self {
        Self {
            seed: 1,
            attempts: 30,
            cells: 25,
            rooms: 11,
            loops: 3,
            organicness: 0.0,
            corridor_width: 1.0,
            hidden: 0.0,
            mix_mode: "transition".into(),
            faction_profile: "industrial_default".into(),
            // Zone order spawn→extraction: prev=start, default=middle, next=end.
            // urban (outlaw) cyberpunk → industrial substrate → priesthood stone.
            prev_faction: "outlaw".into(),
            next_faction: "priesthood".into(),
            default_faction: "industrial_default".into(),
            prev_fraction: 0.25,
            default_fraction: 0.50,
            next_fraction: 0.25,
            auto_regen: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MapGenReport {
    pub spawn: [i32; 2],
    pub end: [i32; 2],
    pub rooms: u32,
    pub pieces: u32,
    pub elapsed_s: f32,
    pub paint: String,
}

#[derive(Resource, Default)]
pub struct MapGenRuntime {
    pub generating: bool,
    pub status: String,
    pub last_report: Option<MapGenReport>,
    pub param_revision: u32,
    pub debounce_until: f32,
    rx: std::sync::Mutex<Option<Receiver<MapGenOutcome>>>,
}

enum MapGenOutcome {
    Ok { report: serde_json::Value },
    Failed(String),
}

#[derive(Component, Clone, Copy)]
pub enum MapGenBtn {
    Regenerate,
    RandomSeed,
    AutoRegenToggle,
    SeedDec,
    SeedInc,
    AttemptsDec,
    AttemptsInc,
    CellsDec,
    CellsInc,
    RoomsDec,
    RoomsInc,
    LoopsDec,
    LoopsInc,
    OrganicDec,
    OrganicInc,
    WidthDec,
    WidthInc,
    HiddenDec,
    HiddenInc,
    MixModeToggle,
    FactionCycle,
    PrevFactionCycle,
    NextFactionCycle,
    DefaultFactionCycle,
    PrevFractionDec,
    PrevFractionInc,
    DefaultFractionDec,
    DefaultFractionInc,
    NextFractionDec,
    NextFractionInc,
}

#[derive(Component)]
pub struct SidebarTabGenerate;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn preview_map_path() -> PathBuf {
    repo_root().join(MAP_GEN_PREVIEW_PATH)
}

fn run_python_preview(settings: &MapGenSettings) -> MapGenOutcome {
    let root = repo_root();
    let script = root.join("tools/gen_maps.py");
    if !script.exists() {
        return MapGenOutcome::Failed(format!("missing {}", script.display()));
    }

    let out = preview_map_path();
    let args = vec![
        script.to_string_lossy().into_owned(),
        "--preview".into(),
        "--no-layout-export".into(),
        "--seed".into(),
        settings.seed.to_string(),
        "--attempts".into(),
        settings.attempts.to_string(),
        "--cells".into(),
        settings.cells.to_string(),
        "--rooms".into(),
        settings.rooms.to_string(),
        "--loops".into(),
        settings.loops.to_string(),
        "--organicness".into(),
        format!("{:.2}", settings.organicness),
        "--corridor-width".into(),
        format!("{:.2}", settings.corridor_width),
        "--hidden".into(),
        format!("{:.2}", settings.hidden),
        "--mix-mode".into(),
        settings.mix_mode.clone(),
        "--faction-profile".into(),
        settings.faction_profile.clone(),
        "--prev-faction".into(),
        settings.prev_faction.clone(),
        "--next-faction".into(),
        settings.next_faction.clone(),
        "--default-faction".into(),
        settings.default_faction.clone(),
        "--prev-fraction".into(),
        format!("{:.2}", settings.prev_fraction),
        "--default-fraction".into(),
        format!("{:.2}", settings.default_fraction),
        "--next-fraction".into(),
        format!("{:.2}", settings.next_fraction),
        "--out".into(),
        out.to_string_lossy().into_owned(),
    ];

    let try_run = |python: &str, extra: &[&str]| -> std::io::Result<std::process::Output> {
        let mut cmd = Command::new(python);
        cmd.args(extra).args(&args).current_dir(&root);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.output()
    };

    let output = try_run("python", &[])
        .or_else(|_| try_run("py", &["-3"]))
        .or_else(|_| try_run("python3", &[]));

    let output = match output {
        Ok(o) => o,
        Err(e) => return MapGenOutcome::Failed(format!("could not run python: {e}")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
        .unwrap_or(stdout.trim());

    let report: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            let hint = if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("{stderr}\n{stdout}")
            };
            return MapGenOutcome::Failed(format!("bad JSON ({e}): {hint}"));
        }
    };

    if !output.status.success() || report.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let err = report
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("generation failed");
        return MapGenOutcome::Failed(err.to_string());
    }

    MapGenOutcome::Ok { report }
}

pub fn request_map_gen(
    settings: &MapGenSettings,
    runtime: &mut MapGenRuntime,
) {
    if runtime.generating {
        return;
    }
    runtime.generating = true;
    runtime.status = "Generating…".into();

    let cfg = settings.clone();
    let (tx, rx) = mpsc::channel();
    *runtime.rx.lock().unwrap() = Some(rx);

    std::thread::spawn(move || {
        let outcome = run_python_preview(&cfg);
        let _ = tx.send(outcome);
    });
}

pub fn map_gen_debounce(
    time: Res<Time>,
    settings: Res<MapGenSettings>,
    mut runtime: ResMut<MapGenRuntime>,
    mut ws: ResMut<EditorWorkspace>,
) {
    if !settings.auto_regen || runtime.generating {
        return;
    }
    if ws.sidebar_tab != SidebarTab::Generate {
        return;
    }
    if time.elapsed_secs() < runtime.debounce_until {
        return;
    }
    if runtime.debounce_until <= 0.0 {
        return;
    }
    runtime.debounce_until = 0.0;
    request_map_gen(&settings, &mut runtime);
    ws.sidebar_dirty = true;
}

pub fn map_gen_poll(
    mut runtime: ResMut<MapGenRuntime>,
    mut settings: ResMut<MapGenSettings>,
    mut ws: ResMut<EditorWorkspace>,
) {
    enum PollMsg {
        StillRunning,
        Disconnected,
        Failed(String),
        Ok(serde_json::Value),
    }

    let msg = {
        let mut slot = runtime.rx.lock().unwrap();
        let Some(rx) = slot.take() else {
            return;
        };
        match rx.try_recv() {
            Err(TryRecvError::Empty) => {
                *slot = Some(rx);
                PollMsg::StillRunning
            }
            Err(TryRecvError::Disconnected) => PollMsg::Disconnected,
            Ok(MapGenOutcome::Failed(msg)) => PollMsg::Failed(msg),
            Ok(MapGenOutcome::Ok { report }) => PollMsg::Ok(report),
        }
    };

    match msg {
        PollMsg::StillRunning => {}
        PollMsg::Disconnected => {
            runtime.generating = false;
            runtime.status = "Generator thread lost".into();
            ws.sidebar_dirty = true;
        }
        PollMsg::Failed(msg) => {
            runtime.generating = false;
            runtime.status = msg;
            ws.sidebar_dirty = true;
        }
        PollMsg::Ok(report) => {
            runtime.generating = false;
            if let Some(seed) = report.get("seed").and_then(|v| v.as_u64()) {
                settings.seed = seed as u32;
            }
            let path = preview_map_path();
            if path.exists() {
                ws.pending_load_map = Some(path);
                ws.pending_map_gen_load = true;
            } else {
                runtime.status = "Preview file missing".into();
            }

            if let (Some(sp), Some(en), Some(rooms), Some(pieces), Some(elapsed)) = (
                report.get("spawn").and_then(|v| v.as_array()),
                report.get("end").and_then(|v| v.as_array()),
                report.get("rooms").and_then(|v| v.as_u64()),
                report.get("pieces").and_then(|v| v.as_u64()),
                report.get("elapsed_s").and_then(|v| v.as_f64()),
            ) {
                let spawn = [
                    sp.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    sp.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                ];
                let end = [
                    en.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                    en.get(1).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                ];
                runtime.last_report = Some(MapGenReport {
                    spawn,
                    end,
                    rooms: rooms as u32,
                    pieces: pieces as u32,
                    elapsed_s: elapsed as f32,
                    paint: report
                        .get("paint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
                runtime.status = format!(
                    "OK {:.1}s · {} rooms · {} pieces",
                    elapsed, rooms, pieces
                );
            } else {
                runtime.status = "Generated".into();
            }
            ws.sidebar_dirty = true;
        }
    }
}

fn bump_params(runtime: &mut MapGenRuntime, time: f32) {
    runtime.param_revision = runtime.param_revision.wrapping_add(1);
    runtime.debounce_until = time + DEBOUNCE_SECS;
}

fn cycle_faction(current: &str) -> String {
    let idx = FACTION_PROFILES
        .iter()
        .position(|p| *p == current)
        .unwrap_or(0);
    FACTION_PROFILES[(idx + 1) % FACTION_PROFILES.len()].into()
}

fn normalize_fractions(prev: &mut f32, default: &mut f32, next: &mut f32) {
    *prev = prev.clamp(0.05, 0.90);
    *default = default.clamp(0.05, 0.90);
    *next = next.clamp(0.05, 0.90);
    let sum = *prev + *default + *next;
    if sum <= f32::EPSILON {
        *prev = 0.25;
        *default = 0.50;
        *next = 0.25;
        return;
    }
    *prev /= sum;
    *default /= sum;
    *next /= sum;
}

fn bump_fraction(settings: &mut MapGenSettings, zone: &str, delta: f32) {
    match zone {
        "prev" => settings.prev_fraction += delta,
        "default" => settings.default_fraction += delta,
        _ => settings.next_fraction += delta,
    }
    normalize_fractions(
        &mut settings.prev_fraction,
        &mut settings.default_fraction,
        &mut settings.next_fraction,
    );
}

pub fn map_gen_button_input(
    time: Res<Time>,
    mut settings: ResMut<MapGenSettings>,
    mut runtime: ResMut<MapGenRuntime>,
    mut ws: ResMut<EditorWorkspace>,
    btns: Query<(&Interaction, &MapGenBtn), Changed<Interaction>>,
    gen_tab: Query<&Interaction, (Changed<Interaction>, With<SidebarTabGenerate>)>,
) {
    if pressed(&gen_tab) {
        crate::editor_sidebar::exit_gallery_mode(&mut ws);
        ws.sidebar_tab = SidebarTab::Generate;
        ws.tool = shared::editor_map::EditorTool::GalleryPreview;
        ws.sidebar_dirty = true;
        if settings.auto_regen && !runtime.generating && runtime.last_report.is_none() {
            bump_params(&mut runtime, time.elapsed_secs());
        }
    }

    for (interaction, btn) in &btns {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *btn {
            MapGenBtn::Regenerate => {
                request_map_gen(&settings, &mut runtime);
            }
            MapGenBtn::RandomSeed => {
                settings.seed = (settings.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)) | 1;
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::AutoRegenToggle => {
                settings.auto_regen = !settings.auto_regen;
            }
            MapGenBtn::SeedDec => {
                settings.seed = settings.seed.saturating_sub(1);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::SeedInc => {
                settings.seed = settings.seed.saturating_add(1);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::AttemptsDec => {
                settings.attempts = settings.attempts.saturating_sub(5).max(5);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::AttemptsInc => {
                settings.attempts = (settings.attempts + 5).min(200);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::CellsDec => {
                settings.cells = settings.cells.saturating_sub(1).max(12);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::CellsInc => {
                settings.cells = (settings.cells + 1).min(40);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::RoomsDec => {
                settings.rooms = settings.rooms.saturating_sub(1).max(2);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::RoomsInc => {
                settings.rooms = (settings.rooms + 1).min(24);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::LoopsDec => {
                settings.loops = settings.loops.saturating_sub(1);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::LoopsInc => {
                settings.loops = (settings.loops + 1).min(8);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::OrganicDec => {
                settings.organicness = (settings.organicness - 0.1).max(0.0);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::OrganicInc => {
                settings.organicness = (settings.organicness + 0.1).min(1.0);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::WidthDec => {
                settings.corridor_width = (settings.corridor_width - 0.1).max(1.0);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::WidthInc => {
                settings.corridor_width = (settings.corridor_width + 0.1).min(2.0);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::HiddenDec => {
                settings.hidden = (settings.hidden - 0.1).max(0.0);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::HiddenInc => {
                settings.hidden = (settings.hidden + 0.1).min(1.0);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::MixModeToggle => {
                settings.mix_mode = if settings.mix_mode == "transition" {
                    "single".into()
                } else {
                    "transition".into()
                };
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::FactionCycle => {
                settings.faction_profile = cycle_faction(&settings.faction_profile);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::PrevFactionCycle => {
                settings.prev_faction = cycle_faction(&settings.prev_faction);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::NextFactionCycle => {
                settings.next_faction = cycle_faction(&settings.next_faction);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::DefaultFactionCycle => {
                settings.default_faction = cycle_faction(&settings.default_faction);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::PrevFractionDec => {
                bump_fraction(&mut settings, "prev", -0.05);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::PrevFractionInc => {
                bump_fraction(&mut settings, "prev", 0.05);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::DefaultFractionDec => {
                bump_fraction(&mut settings, "default", -0.05);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::DefaultFractionInc => {
                bump_fraction(&mut settings, "default", 0.05);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::NextFractionDec => {
                bump_fraction(&mut settings, "next", -0.05);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::NextFractionInc => {
                bump_fraction(&mut settings, "next", 0.05);
                bump_params(&mut runtime, time.elapsed_secs());
            }
        }
        ws.sidebar_dirty = true;
    }
}

fn pressed(q: &Query<&Interaction, (Changed<Interaction>, impl bevy::ecs::query::QueryFilter + 'static)>) -> bool {
    q.iter().any(|i| *i == Interaction::Pressed)
}

pub fn spawn_map_gen_panel(
    parent: &mut ChildSpawnerCommands,
    settings: &MapGenSettings,
    runtime: &MapGenRuntime,
) {
    parent.spawn((
        Text::new("Procedural map"),
        TextFont { font_size: 13.0, ..default() },
        TextColor(Color::srgb(0.45, 0.92, 1.0)),
    ));

    parent.spawn((
        Text::new(runtime.status.as_str()),
        TextFont { font_size: 11.5, ..default() },
        TextColor(if runtime.generating {
            Color::srgb(1.0, 0.85, 0.35)
        } else if runtime.last_report.is_some() {
            Color::srgb(0.45, 1.0, 0.55)
        } else {
            Color::srgb(0.55, 0.62, 0.72)
        }),
    ));

    row_btn(parent, MapGenBtn::Regenerate, "Regenerate now");
    row_btn(
        parent,
        MapGenBtn::AutoRegenToggle,
        if settings.auto_regen { "Auto-regen: ON" } else { "Auto-regen: OFF" },
    );

    section_label(parent, "Composition");
    row_btn(
        parent,
        MapGenBtn::MixModeToggle,
        if settings.mix_mode == "transition" {
            "Mode: transition (start/mid/end)"
        } else {
            "Mode: single profile"
        },
    );
    if settings.mix_mode == "single" {
        row_btn(
            parent,
            MapGenBtn::FactionCycle,
            &format!("Profile: {}", settings.faction_profile),
        );
    } else {
        row_btn(
            parent,
            MapGenBtn::PrevFactionCycle,
            &format!("Start: {}", settings.prev_faction),
        );
        row_btn(
            parent,
            MapGenBtn::DefaultFactionCycle,
            &format!("Middle: {}", settings.default_faction),
        );
        row_btn(
            parent,
            MapGenBtn::NextFactionCycle,
            &format!("End: {}", settings.next_faction),
        );
        slider_row(
            parent,
            "Start zone",
            &format!("{:.0}%", settings.prev_fraction * 100.0),
            settings.prev_fraction,
            MapGenBtn::PrevFractionDec,
            MapGenBtn::PrevFractionInc,
        );
        slider_row(
            parent,
            "Middle zone",
            &format!("{:.0}%", settings.default_fraction * 100.0),
            settings.default_fraction,
            MapGenBtn::DefaultFractionDec,
            MapGenBtn::DefaultFractionInc,
        );
        slider_row(
            parent,
            "End zone",
            &format!("{:.0}%", settings.next_fraction * 100.0),
            settings.next_fraction,
            MapGenBtn::NextFractionDec,
            MapGenBtn::NextFractionInc,
        );
    }

    section_label(parent, "Layout");
    param_row(parent, "Seed", &settings.seed.to_string(), MapGenBtn::SeedDec, MapGenBtn::SeedInc);
    parent.spawn((MapGenBtn::RandomSeed, small_btn("Randomize seed")));
    param_row(parent, "Grid (cells)", &settings.cells.to_string(), MapGenBtn::CellsDec, MapGenBtn::CellsInc);
    param_row(parent, "Max rooms", &settings.rooms.to_string(), MapGenBtn::RoomsDec, MapGenBtn::RoomsInc);
    param_row(parent, "Loops", &settings.loops.to_string(), MapGenBtn::LoopsDec, MapGenBtn::LoopsInc);

    section_label(parent, "Feel");
    slider_row(parent, "Organicness", &format!("{:.1}", settings.organicness),
               settings.organicness, MapGenBtn::OrganicDec, MapGenBtn::OrganicInc);
    slider_row(parent, "Corridor width", &format!("{:.1}", settings.corridor_width),
               settings.corridor_width - 1.0, MapGenBtn::WidthDec, MapGenBtn::WidthInc);

    section_label(parent, "Secrets");
    slider_row(parent, "Hidden areas", &format!("{:.1}", settings.hidden),
               settings.hidden, MapGenBtn::HiddenDec, MapGenBtn::HiddenInc);

    section_label(parent, "Advanced");
    param_row(parent, "Attempts", &settings.attempts.to_string(), MapGenBtn::AttemptsDec, MapGenBtn::AttemptsInc);

    if let Some(r) = &runtime.last_report {
        parent.spawn((
            Text::new(format!(
                "spawn {:?}  end {:?}\n{} rooms · {} pcs · {:.1}s",
                r.spawn, r.end, r.rooms, r.pieces, r.elapsed_s
            )),
            TextFont { font_size: 10.5, ..default() },
            TextColor(Color::srgb(0.65, 0.72, 0.82)),
        ));
        if !r.paint.is_empty() {
            parent.spawn((
                Text::new(r.paint.as_str()),
                TextFont { font_size: 9.5, ..default() },
                TextColor(Color::srgb(0.5, 0.58, 0.68)),
            ));
        }
    }

    parent.spawn((
        Text::new("G = playtest after regen\nAuto-regen waits ~0.75s"),
        TextFont { font_size: 10.0, ..default() },
        TextColor(Color::srgb(0.45, 0.52, 0.62)),
    ));
}

fn section_label(parent: &mut ChildSpawnerCommands, text: &str) {
    parent.spawn((
        Text::new(text),
        TextFont { font_size: 11.0, ..default() },
        TextColor(Color::srgb(0.40, 0.86, 1.0)),
        Node { margin: UiRect::top(Val::Px(7.0)), ..default() },
    ));
}

/// Param row with a slider-style fill bar (`fill` is 0..1 of the track).
fn slider_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: &str,
    fill: f32,
    dec: MapGenBtn,
    inc: MapGenBtn,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            margin: UiRect::vertical(Val::Px(3.0)),
            ..default()
        })
        .with_children(|col| {
            col.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Text::new(label),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(Color::srgb(0.55, 0.65, 0.75)),
                    Node { width: Val::Px(96.0), ..default() },
                ));
                row.spawn((dec, small_btn("−")));
                row.spawn((
                    Text::new(value),
                    TextFont { font_size: 11.0, ..default() },
                    TextColor(Color::srgb(0.85, 0.95, 1.0)),
                    Node { width: Val::Px(34.0), justify_content: JustifyContent::Center, ..default() },
                ));
                row.spawn((inc, small_btn("+")));
            });
            col.spawn((
                Node { width: Val::Percent(100.0), height: Val::Px(4.0), ..default() },
                BackgroundColor(Color::srgba(0.10, 0.16, 0.24, 0.95)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Node {
                        width: Val::Percent(fill.clamp(0.0, 1.0) * 100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.30, 0.75, 0.95)),
                ));
            });
        });
}

fn row_btn(parent: &mut ChildSpawnerCommands, action: MapGenBtn, label: &str) {
    parent.spawn((
        action,
        Button,
        Node {
            padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            margin: UiRect::vertical(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.12, 0.22, 0.34, 0.98)),
        Text::new(label),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.92, 1.0)),
    ));
}

fn param_row(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    value: &str,
    dec: MapGenBtn,
    inc: MapGenBtn,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(4.0),
            margin: UiRect::vertical(Val::Px(2.0)),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.55, 0.65, 0.75)),
                Node {
                    width: Val::Px(88.0),
                    ..default()
                },
            ));
            row.spawn((dec, small_btn("−")));
            row.spawn((
                Text::new(value),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.95, 1.0)),
                Node {
                    width: Val::Px(36.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ));
            row.spawn((inc, small_btn("+")));
        });
}

fn small_btn(label: &str) -> impl Bundle {
    (
        Button,
        Node {
            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.18, 0.28, 0.95)),
        Text::new(label),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.45, 0.92, 1.0)),
    )
}
