//! Run loop: stretch extraction, hub shops, route picks, permadeath.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::items::{self, FLASHLIGHT, MAP, PIPE_BAT};
use shared::level as shared_level;
use shared::run::{self, CampKind, RunPhase, RouteOption, RunState};
use shared::protocol::{NetTransform, Player, PlayerAlive, PlayerName};

use crate::character::CharacterSystems;
use crate::combat::Health;
use crate::items::Inventory;
use crate::level::{LevelEntity, LoadedLevel, load_level};
use crate::players::{LatestInput, PlayerOwner};

pub struct RunPlugin;

impl Plugin for RunPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_run_entity)
            .add_systems(
                FixedUpdate,
                (
                    check_extraction,
                    hub_shop,
                    hub_routing,
                    check_permadeath,
                    restart_run_keys,
                )
                    .chain()
                    .after(CharacterSystems)
                    .run_if(in_state(ClientState::Disconnected)),
            );
    }
}

#[derive(Component)]
pub struct RunEntity;

fn spawn_run_entity(mut commands: Commands) {
    let start = run::start_node();
    let run_seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs().wrapping_mul(0x9e3779b97f4a7c15))
        .unwrap_or(0xDEAD_BEEF_1234_5678);
    commands.spawn((
        Replicated,
        RunEntity,
        RunState {
            phase: RunPhase::InStretch,
            level_id: start.id.to_string(),
            hub_id: None,
            credits: 0,
            scrap: 0,
            map_holder: None,
            route_options: vec![],
            run_seed,
        },
    ));
}

fn transition_level(
    commands: &mut Commands,
    level_entities: &Query<Entity, With<LevelEntity>>,
    loaded: &mut LoadedLevel,
    run: &mut RunState,
    level_id: &str,
    phase: RunPhase,
) {
    let def = shared_level::level_by_id(level_id, run.run_seed);
    load_level(commands, level_entities, &def);
    loaded.id = level_id.to_string();
    run.phase = phase;
    run.level_id = level_id.to_string();
}

fn spawn_position(level_id: &str, index: usize, seed: u64) -> Vec3 {
    let def = shared_level::level_by_id(level_id, seed);
    def.player_spawns[index % def.player_spawns.len()]
        + Vec3::Y
            * (shared::config::PLAYER_CAPSULE_LENGTH / 2.0 + shared::config::PLAYER_CAPSULE_RADIUS)
}

fn apply_alive_spawn(
    level_id: &str,
    index: usize,
    seed: u64,
    transform: &mut Transform,
    net: &mut NetTransform,
    health: &mut Health,
    alive: &mut PlayerAlive,
) {
    if !alive.0 {
        return;
    }
    let pos = spawn_position(level_id, index, seed);
    transform.translation = pos;
    net.translation = pos;
    health.current = health.max;
    alive.0 = true;
}

fn build_routes(node: &run::StretchNode) -> Vec<RouteOption> {
    node.routes
        .iter()
        .filter_map(|(target, label, cost)| {
            run::node(target).map(|n| RouteOption {
                target_id: target.to_string(),
                label: format!("{label} → {}", n.label),
                camp: n.camp,
                cost: *cost,
            })
        })
        .collect()
}

fn check_extraction(
    mut run_q: Query<&mut RunState, With<RunEntity>>,
    players: Query<(&Transform, &PlayerAlive), With<Player>>,
) {
    let Ok(mut run) = run_q.single_mut() else { return };
    if run.phase != RunPhase::InStretch { return }

    let def = shared_level::level_by_id(&run.level_id, run.run_seed);
    if def.extraction.is_none() { return }

    let any_alive = players.iter().any(|(_, a)| a.0);
    if !any_alive { return }

    // Hub floor at y = -4.0; players are "in hub" once below the extraction floor (y < -1.5).
    const HUB_ENTRY_Y: f32 = -1.5;
    let all_in_hub = players.iter()
        .filter(|(_, a)| a.0)
        .all(|(t, _)| t.translation.y < HUB_ENTRY_Y);
    if !all_in_hub { return }

    let node   = run::node(&run.level_id).unwrap_or(run::start_node());
    let hub_id = node.routes.first().map(|r| r.0).unwrap_or("hub_medbay");
    let routes = build_routes(run::node(hub_id).unwrap_or(node));
    info!("party entered hub '{hub_id}' via drop shaft");

    // Hub geometry is embedded in the stretch level — just flip state.
    // DO NOT change level_id: that would cause the client to reload visuals.
    run.phase         = RunPhase::InHub;
    run.hub_id        = Some(hub_id.to_string());
    run.route_options = routes;
}

fn shop_stock(camp: CampKind) -> Vec<(u32, u32, &'static str)> {
    // (item_id, cost, log name)
    let mut stock = vec![
        (FLASHLIGHT, 12, "Flashlight"),
        (PIPE_BAT, 10, "Pipe Bat"),
    ];
    match camp {
        CampKind::MedBay => stock.push((FLASHLIGHT, 8, "Tactical Light")),
        CampKind::Armory => stock.push((PIPE_BAT, 8, "Reinforced Bat")),
        CampKind::Workshop => stock.push((FLASHLIGHT, 6, "Helmet Mount Kit")),
        CampKind::Intel => stock.push((MAP, 12, "Sector Map")),
    }
    if !stock.iter().any(|(id, _, _)| *id == MAP) {
        stock.push((MAP, 20, "Sector Map"));
    }
    stock
}

fn hub_shop(
    mut run_q: Query<&mut RunState, With<RunEntity>>,
    mut players: Query<
        (
            &mut LatestInput,
            &mut Inventory,
            &PlayerName,
            &PlayerOwner,
            &PlayerAlive,
        ),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<shared::protocol::InventoryUpdate>>,
) {
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    if run.phase != RunPhase::InHub {
        return;
    }
    let camp = run.hub_id.as_deref()
        .and_then(run::node)
        .map(|n| n.camp)
        .unwrap_or(CampKind::MedBay);
    let stock = shop_stock(camp);

    for (mut input, mut inventory, name, owner, alive) in &mut players {
        if !alive.0 {
            continue;
        }
        let Some(buy_id) = input.0.shop_buy.take() else {
            continue;
        };
        let Some((_, cost, label)) = stock.iter().find(|(id, _, _)| *id == buy_id) else {
            continue;
        };
        if run.credits < *cost {
            info!("not enough credits for {label}");
            continue;
        }
        if buy_id == MAP {
            if run.map_holder.is_some() {
                info!("someone already carries the map");
                continue;
            }
            run.map_holder = Some(name.0.clone());
        }
        let free = inventory.0.iter().position(Option::is_none);
        let Some(slot) = free else {
            continue;
        };
        let item = match buy_id {
            FLASHLIGHT => items::flashlight(),
            MAP => items::map(),
            PIPE_BAT => items::pipe_bat(),
            _ => continue,
        };
        run.credits -= *cost;
        inventory.0[slot] = Some(item);
        info!("{} bought {label} for {cost}c", name.0);
        writer.write(ToClients {
            targets: SendTargets::Single(owner.0),
            message: shared::protocol::InventoryUpdate {
                slots: inventory.0.clone(),
            },
        });
    }
}

fn hub_routing(
    mut commands: Commands,
    level_entities: Query<Entity, With<LevelEntity>>,
    mut loaded: ResMut<LoadedLevel>,
    mut run_q: Query<&mut RunState, With<RunEntity>>,
    mut players: Query<
        (
            Entity,
            &mut Transform,
            &mut NetTransform,
            &mut Health,
            &mut PlayerAlive,
            &mut LatestInput,
        ),
        With<Player>,
    >,
) {
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    if run.phase != RunPhase::InHub {
        return;
    }
    let mut picked = None;
    for (_, _, _, _, _, mut input) in &mut players {
        if let Some(idx) = input.0.route_select.take() {
            picked = Some(idx);
            break;
        }
    }
    let Some(idx) = picked else {
        return;
    };
    let route = run.route_options.get(idx as usize).cloned();
    let Some(route) = route else {
        return;
    };
    if run.credits < route.cost {
        info!("not enough credits for route (need {})", route.cost);
        return;
    }
    run.credits -= route.cost;
    info!("departing for {}", route.label);
    let target_def = shared_level::level_by_id(&route.target_id, run.run_seed);
    let target_id  = route.target_id.clone();

    if target_def.extraction.is_some() {
        // Target is a sewer stretch — load it and clear hub state.
        run.hub_id = None;
        transition_level(
            &mut commands,
            &level_entities,
            &mut loaded,
            &mut run,
            &target_id,
            RunPhase::InStretch,
        );
        let seed = run.run_seed;
        for (i, (_, mut transform, mut net, mut health, mut alive, _)) in players.iter_mut().enumerate() {
            apply_alive_spawn(&target_id, i, seed, &mut transform, &mut net, &mut health, &mut alive);
        }
    } else {
        // Hub-to-hub transition: stay in the same level, just switch camp metadata.
        // No level reload → client visuals stay intact.
        run.hub_id = Some(target_id.clone());
        run.phase  = RunPhase::InHub;
        if let Some(node) = run::node(&target_id) {
            run.route_options = build_routes(node);
        }
    }
}

fn check_permadeath(mut run_q: Query<&mut RunState, With<RunEntity>>, alive: Query<&PlayerAlive, With<Player>>) {
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    if run.phase == RunPhase::RunOver {
        return;
    }
    let any_alive = alive.iter().any(|a| a.0);
    let any_players = !alive.is_empty();
    if any_players && !any_alive {
        run.phase = RunPhase::RunOver;
        info!("RUN OVER — all operators down");
    }
}

fn restart_run_keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut commands: Commands,
    level_entities: Query<Entity, With<LevelEntity>>,
    mut loaded: ResMut<LoadedLevel>,
    mut run_q: Query<&mut RunState, With<RunEntity>>,
    mut players: Query<
        (
            Entity,
            &mut Transform,
            &mut NetTransform,
            &mut Health,
            &mut PlayerAlive,
            &PlayerOwner,
            &mut Inventory,
        ),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<shared::protocol::InventoryUpdate>>,
) {
    let Some(keys) = keys else { return };
    if !keys.just_pressed(KeyCode::KeyR) {
        return;
    }
    let Ok(mut run) = run_q.single_mut() else {
        return;
    };
    if run.phase != RunPhase::RunOver {
        return;
    }
    let start = run::start_node();
    run.credits = 0;
    run.scrap = 0;
    run.map_holder = None;
    run.hub_id = None;
    // Fresh seed gives a new layout on restart.
    run.run_seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs().wrapping_mul(0x9e3779b97f4a7c15))
        .unwrap_or(run.run_seed.wrapping_add(1));
    transition_level(
        &mut commands,
        &level_entities,
        &mut loaded,
        &mut run,
        start.id,
        RunPhase::InStretch,
    );
    for (i, (entity, mut transform, mut net, mut health, mut alive, owner, mut inventory)) in
        players.iter_mut().enumerate()
    {
        if alive.0 {
            apply_alive_spawn(start.id, i, run.run_seed, &mut transform, &mut net, &mut health, &mut alive);
            continue;
        }
        alive.0 = true;
        health.current = health.max;
        inventory.0.fill(None);
        let pos = spawn_position(start.id, i, run.run_seed);
        transform.translation = pos;
        net.translation = pos;
        commands.entity(entity).insert((
            Transform::from_translation(pos),
            Visibility::default(),
        ));
        writer.write(ToClients {
            targets: SendTargets::Single(owner.0),
            message: shared::protocol::InventoryUpdate {
                slots: inventory.0.clone(),
            },
        });
    }
}
