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

#[derive(Resource, Clone, Debug)]
pub struct MapGenSettings {
    pub seed: u32,
    pub attempts: u32,
    pub synth_retries: u32,
    pub target_degree: f32,
    pub auto_regen: bool,
}

impl Default for MapGenSettings {
    fn default() -> Self {
        Self {
            seed: 1,
            attempts: 30,
            synth_retries: 12,
            target_degree: 2.25,
            auto_regen: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MapGenReport {
    pub spawn: [i32; 2],
    pub end: [i32; 2],
    pub avg_degree: f32,
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
    SynthDec,
    SynthInc,
    DegreeDec,
    DegreeInc,
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
    let mut args = vec![
        script.to_string_lossy().into_owned(),
        "--preview".into(),
        "--no-layout-export".into(),
        "--seed".into(),
        settings.seed.to_string(),
        "--attempts".into(),
        settings.attempts.to_string(),
        "--synth-retries".into(),
        settings.synth_retries.to_string(),
        "--target-degree".into(),
        format!("{:.2}", settings.target_degree),
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

            if let (Some(sp), Some(en), Some(avg), Some(pieces), Some(elapsed)) = (
                report.get("spawn").and_then(|v| v.as_array()),
                report.get("end").and_then(|v| v.as_array()),
                report.get("avg_degree").and_then(|v| v.as_f64()),
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
                    avg_degree: avg as f32,
                    pieces: pieces as u32,
                    elapsed_s: elapsed as f32,
                    paint: report
                        .get("paint")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
                runtime.status = format!(
                    "OK {:.1}s · {} pieces · deg {:.2}",
                    elapsed, pieces, avg
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
            MapGenBtn::SynthDec => {
                settings.synth_retries = settings.synth_retries.saturating_sub(2).max(2);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::SynthInc => {
                settings.synth_retries = (settings.synth_retries + 2).min(40);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::DegreeDec => {
                settings.target_degree = (settings.target_degree - 0.1).max(1.5);
                bump_params(&mut runtime, time.elapsed_secs());
            }
            MapGenBtn::DegreeInc => {
                settings.target_degree = (settings.target_degree + 0.1).min(3.5);
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
        Text::new("Procedural map (5×5)"),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.45, 0.92, 1.0)),
    ));

    parent.spawn((
        Text::new(runtime.status.as_str()),
        TextFont {
            font_size: 11.5,
            ..default()
        },
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
        if settings.auto_regen {
            "Auto-regen: ON"
        } else {
            "Auto-regen: OFF"
        },
    );

    param_row(parent, "Seed", &settings.seed.to_string(), MapGenBtn::SeedDec, MapGenBtn::SeedInc);
    parent.spawn((MapGenBtn::RandomSeed, small_btn("Randomize seed")));

    param_row(
        parent,
        "Attempts",
        &settings.attempts.to_string(),
        MapGenBtn::AttemptsDec,
        MapGenBtn::AttemptsInc,
    );
    param_row(
        parent,
        "Synth retries",
        &settings.synth_retries.to_string(),
        MapGenBtn::SynthDec,
        MapGenBtn::SynthInc,
    );
    param_row(
        parent,
        "Target degree",
        &format!("{:.2}", settings.target_degree),
        MapGenBtn::DegreeDec,
        MapGenBtn::DegreeInc,
    );

    if let Some(r) = &runtime.last_report {
        parent.spawn((
            Text::new(format!(
                "spawn {:?}  end {:?}\n{} pcs  deg {:.2}  {:.1}s",
                r.spawn, r.end, r.pieces, r.avg_degree, r.elapsed_s
            )),
            TextFont {
                font_size: 10.5,
                ..default()
            },
            TextColor(Color::srgb(0.65, 0.72, 0.82)),
        ));
        if !r.paint.is_empty() {
            parent.spawn((
                Text::new(r.paint.as_str()),
                TextFont {
                    font_size: 9.5,
                    ..default()
                },
                TextColor(Color::srgb(0.5, 0.58, 0.68)),
            ));
        }
    }

    parent.spawn((
        Text::new("G = playtest after regen\nAdjust sliders — auto-regen waits ~0.75s"),
        TextFont {
            font_size: 10.0,
            ..default()
        },
        TextColor(Color::srgb(0.45, 0.52, 0.62)),
    ));
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
