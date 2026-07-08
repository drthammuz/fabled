//! Server-side player lifecycle. Movement is handled by the kinematic
//! character controller in `character.rs`; clients only send `PlayerInput`.

use avian3d::math::AdjustPrecision;
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::level;
use shared::protocol::{
    ClassPick, InventoryUpdate, NetTransform, Player, PlayerAlive, PlayerClass, PlayerHealth,
    PlayerInput, PlayerName, YouAre,
};
use bevy::ecs::schedule::common_conditions::not;
use shared::EditorMode;
use shared::CityViewMode;
use shared::{classes, items, KenneyPlaytestGeneration, TestMapStyle, TestMode};
use shared::map_pool;

use crate::combat::Health;
use crate::level::LevelReady;

use crate::character::{
    CharacterCollisions, CharacterController, CoyoteTime, CrouchState, GroundContact,
    GroundDetection, PlayerWaterContact,
    SpeedMultiplier,
};

pub struct ServerPlayersPlugin;

impl Plugin for ServerPlayersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpawnCounter>()
            .add_plugins(super::character::CharacterControllerPlugin)
            .add_observer(on_client_connected)
            .add_observer(on_client_disconnected)
            .add_systems(
                FixedUpdate,
                (
                    handle_class_pick,
                    test_respawn,
                )
                    .run_if(in_state(ClientState::Disconnected)),
            )
            .add_systems(
                FixedLast,
                sync_net_transforms.run_if(in_state(ClientState::Disconnected)),
            )
            .add_systems(
                PostStartup,
                spawn_local_player
                    .after(LevelReady)
                    .run_if(not(resource_exists::<EditorMode>))
                    .run_if(not(resource_exists::<shared::HeadlessServer>)),
            )
            .add_systems(
                FixedUpdate,
                editor_playtest_player
                    .run_if(in_state(ClientState::Disconnected))
                    .after(test_respawn),
            );
    }
}

/// Spawned only during in-process editor playtest; despawned when leaving playtest.
#[derive(Component)]
pub(crate) struct EditorPlaytestPlayer;

/// Cycles through level player spawn points by join order.
#[derive(Resource, Default)]
pub struct SpawnCounter(usize);

/// Which client owns this player entity. Server-only, never replicated.
#[derive(Component)]
pub struct PlayerOwner(pub ClientId);

/// Latest movement intent received from the owning client. Server-only.
#[derive(Component, Default)]
pub struct LatestInput(pub PlayerInput);

/// Spawns the player entity for the local (listen-server) participant.
pub fn spawn_local_player(
    mut commands: Commands,
    mut counter: ResMut<SpawnCounter>,
    test: Option<Res<shared::TestMode>>,
    city: Option<Res<shared::CityViewMode>>,
) {
    let player = spawn_player(
        &mut commands,
        &mut counter,
        ClientId::Server,
        "Host".into(),
        test.as_deref(),
        city.as_deref(),
        false,
    );
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(ClientId::Server),
        message: YouAre { player },
    });
}

fn on_client_connected(
    add: On<Add, AuthorizedClient>,
    mut commands: Commands,
    mut counter: ResMut<SpawnCounter>,
    test: Option<Res<shared::TestMode>>,
    city: Option<Res<shared::CityViewMode>>,
) {
    let client_entity = add.entity;
    let client_id = ClientId::Client(client_entity);
    let name = format!("Player {}", counter.0 + 1);
    info!("client {client_entity} connected, spawning '{name}'");
    let player = spawn_player(
        &mut commands,
        &mut counter,
        client_id,
        name,
        test.as_deref(),
        city.as_deref(),
        false,
    );
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client_id),
        message: YouAre { player },
    });
}

fn on_client_disconnected(
    remove: On<Remove, ConnectedClient>,
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerOwner, &Transform, &mut super::items::Inventory)>,
) {
    let client_id = ClientId::Client(remove.entity);
    for (entity, owner, transform, mut inventory) in &mut players {
        if owner.0 != client_id {
            continue;
        }
        // Spill carried loot into the world so it isn't lost for the team.
        // The grab (if any) releases by itself: the force-applying system
        // stops running once the player entity is gone.
        let mut dropped = 0;
        for (slot, item) in inventory.0.iter_mut().enumerate() {
            let Some(item) = item.take() else {
                continue;
            };
            let angle = slot as f32 / config::INVENTORY_SLOTS as f32 * std::f32::consts::TAU;
            let offset = Vec3::new(angle.cos(), 1.0, angle.sin()) * 0.6;
            super::level::spawn_world_item(
                &mut commands,
                item,
                transform.translation + offset,
                Vec3::ZERO,
                false,
            );
            dropped += 1;
        }
        info!(
            "client disconnected, despawning player {entity} ({dropped} items dropped)"
        );
        commands.entity(entity).despawn();
    }
}

fn capsule_center_on_floor(x: f32, z: f32, floor_top: f32) -> Vec3 {
    Vec3::new(
        x,
        floor_top + config::PLAYER_CAPSULE_LENGTH / 2.0 + config::PLAYER_CAPSULE_RADIUS,
        z,
    )
}

fn city_spawn_pos(index: usize) -> Vec3 {
    let spread = Vec3::new((index % 2) as f32 * 2.0 - 1.0, 0.0, (index / 2) as f32 * 2.0);
    config::CITY_SPAWN + spread
}

fn testmap_spawn_pos(index: usize, test: Option<&TestMode>, editor_active: bool) -> Vec3 {
    if let Some(test) = test {
        if test.style == TestMapStyle::Kenney {
            if let Some([x, z]) = map_pool::play_spawn_xz(editor_active, index) {
                let floor_y = map_pool::play_spawn_y(editor_active);
                return capsule_center_on_floor(x, z, floor_y);
            }
            let layout = map_pool::play_layout(editor_active);
            if !layout.pieces.is_empty() {
                let focus = layout.focus_xz();
                let spread = Vec3::new((index % 2) as f32 * 2.0 - 1.0, 0.0, (index / 2) as f32 * 2.0);
                return capsule_center_on_floor(focus.x + spread.x, focus.y + spread.z, 0.0);
            }
        }
        let (cx, cz) = level::kenney_sandbox_center_xz();
        let spread = Vec3::new((index % 2) as f32 * 2.0 - 1.0, 0.0, (index / 2) as f32 * 2.0);
        return capsule_center_on_floor(cx + spread.x, cz - 6.0 + spread.z, 0.0);
    }
    let spawns = level::active_level().player_spawns;
    spawns[index % spawns.len()]
}

fn spawn_player(
    commands: &mut Commands,
    counter: &mut SpawnCounter,
    owner: ClientId,
    name: String,
    test: Option<&shared::TestMode>,
    city: Option<&shared::CityViewMode>,
    editor_active: bool,
) -> Entity {
    let base = if city.is_some() {
        city_spawn_pos(counter.0)
    } else {
        testmap_spawn_pos(counter.0, test, editor_active)
    };
    let spawn_pos = if test.is_some() || city.is_some() {
        base
    } else {
        base + Vec3::Y * (config::PLAYER_CAPSULE_LENGTH / 2.0 + config::PLAYER_CAPSULE_RADIUS)
    };
    counter.0 += 1;

    commands
        .spawn((
            (
                Replicated,
                Player,
                CharacterController,
                GroundDetection::default(),
                CharacterCollisions::default(),
                super::grab::GrabTarget::default(),
                PlayerName(name),
                PlayerAlive(true),
                PlayerClass::default(),
                PlayerOwner(owner),
                LatestInput::default(),
                SpeedMultiplier::default(),
                PlayerWaterContact::default(),
                Mass(config::PLAYER_MASS),
                LinearVelocity::default(),
            ),
            (
                shared::protocol::PlayerHeldKind::default(),
                shared::protocol::PlayerAttackAnim::default(),
                NetTransform {
                    translation: spawn_pos,
                    rotation: Quat::IDENTITY,
                },
                Collider::cuboid(
                    config::PLAYER_BODY_WIDTH,
                    config::PLAYER_BODY_HEIGHT,
                    config::PLAYER_BODY_WIDTH,
                ),
                Transform::from_translation(spawn_pos),
            ),
        ))
        .insert((
            CrouchState::default(),
            GroundContact::default(),
            CoyoteTime(config::PLAYER_COYOTE_TIME),
            crate::combat::AttackCooldown::default(),
            crate::combat::PlayerArmor::default(),
            crate::npc::HackCooldown::default(),
            // Dev loadout so every spawn path (editor playtest included, which
            // never goes through class pick) can fight: bat + scrap pistol.
            // handle_class_pick rebuilds the inventory with the class loadout.
            super::items::Inventory({
                let mut slots = vec![None; config::INVENTORY_SLOTS];
                slots[0] = Some(items::pipe_bat());
                if slots.len() > 1 {
                    slots[1] = Some(items::scrap_pistol());
                }
                slots
            }),
            Health::default(),
            PlayerHealth::default(),
        ))
        .id()
}

fn handle_class_pick(
    mut picks: MessageReader<FromClient<ClassPick>>,
    mut players: Query<
        (
            &PlayerOwner,
            &mut PlayerClass,
            &mut SpeedMultiplier,
            &mut super::items::Inventory,
            &mut Health,
            &PlayerAlive,
        ),
        With<Player>,
    >,
    mut writer: MessageWriter<ToClients<InventoryUpdate>>,
) {
    for FromClient { client_id, message } in picks.read() {
        let ClassPick(kind) = *message;
        let def = classes::class_def(kind);
        for (owner, mut class, mut speed, mut inv, mut health, alive) in &mut players {
            if owner.0 != *client_id {
                continue;
            }
            if !alive.0 {
                continue;
            }
            class.0 = kind;
            speed.0 = def.speed_mult;
            health.max = def.max_hp;
            health.current = def.max_hp;
            // Per-class inventory (option-1 loadout): size to the class's own
            // slot count and grant ONLY its own starter — no universal scrap
            // pistol. That leaves a free slot so the player can always buy from a
            // vendor (the earlier "can't buy until you sell" bug was the pistol
            // filling the last slot). The real per-class length is sent as-is;
            // the client hotbar hides slots beyond it.
            inv.0 = vec![None; def.inventory_slots.max(def.starting_items.len())];
            for (slot, &item_id) in def.starting_items.iter().enumerate() {
                inv.0[slot] = items::by_id(item_id);
            }
            writer.write(ToClients {
                targets: SendTargets::Single(owner.0),
                message: InventoryUpdate { slots: inv.0.clone() },
            });
            info!("player {:?} chose {:?}", client_id, kind);
        }
    }
}

fn sync_net_transforms(mut query: Query<(&Transform, &mut NetTransform)>) {
    for (transform, mut net) in &mut query {
        net.set_if_neq(NetTransform {
            translation: transform.translation,
            rotation: transform.rotation,
        });
    }
}

/// Y below this → auto-respawn in developer test maps.
const TEST_RESPAWN_Y: f32 = -12.0;

/// Seconds a dead player waits before the dev auto-respawn kicks in.
const DEV_RESPAWN_SECS: f32 = 3.0;

fn test_respawn(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    test: Option<Res<TestMode>>,
    city: Option<Res<CityViewMode>>,
    editor: Option<Res<EditorMode>>,
    mut dead_for: Local<f32>,
    mut players: Query<
        (
            &mut Transform,
            &mut NetTransform,
            &mut LinearVelocity,
            &mut PlayerAlive,
            &mut Health,
        ),
        With<CharacterController>,
    >,
) {
    if test.is_none() && city.is_none() {
        return;
    }
    let r_pressed = keys
        .as_ref()
        .is_some_and(|k| k.just_pressed(KeyCode::KeyR));

    let mut any_dead = false;
    for (i, (mut transform, mut net, mut vel, mut alive, mut health)) in
        players.iter_mut().enumerate()
    {
        // Dead players auto-respawn in dev maps (a silently dead player is
        // invisible to enemy perception → "nothing ever aggros me again").
        if !alive.0 {
            any_dead = true;
            if !r_pressed && *dead_for < DEV_RESPAWN_SECS {
                continue;
            }
            alive.0 = true;
            health.current = health.max;
        } else {
            let fell = transform.translation.y < TEST_RESPAWN_Y;
            if !r_pressed && !fell {
                continue;
            }
        }
        let pos = if city.is_some() {
            city_spawn_pos(i)
        } else {
            testmap_spawn_pos(i, test.as_deref(), editor.is_some())
        };
        transform.translation = pos;
        net.translation = pos;
        vel.0 = Vec3::ZERO.adjust_precision();
        info!("test respawn → ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z);
    }
    *dead_for = if any_dead {
        *dead_for + time.delta_secs()
    } else {
        0.0
    };
}

fn editor_playtest_player(
    editor: Option<Res<EditorMode>>,
    test: Option<Res<TestMode>>,
    generation: Res<KenneyPlaytestGeneration>,
    mut last_gen: Local<u32>,
    mut commands: Commands,
    mut counter: ResMut<SpawnCounter>,
    mut inv_writer: MessageWriter<ToClients<InventoryUpdate>>,
    playtest: Query<Entity, With<EditorPlaytestPlayer>>,
    mut transforms: Query<(&mut Transform, &mut NetTransform), With<EditorPlaytestPlayer>>,
) {
    if editor.is_none() {
        for e in playtest.iter() {
            commands.entity(e).despawn();
        }
        *last_gen = generation.0;
        return;
    }

    if *last_gen == generation.0 {
        return;
    }
    *last_gen = generation.0;

    let kenney = test
        .as_ref()
        .is_some_and(|t| t.style == TestMapStyle::Kenney);

    if kenney {
        if playtest.is_empty() {
            let player = spawn_player(
                &mut commands,
                &mut counter,
                ClientId::Server,
                "Host".into(),
                test.as_deref(),
                None,
                true,
            );
            commands.entity(player).insert(EditorPlaytestPlayer);
            commands.server_trigger(ToClients {
                targets: SendTargets::Single(ClientId::Server),
                message: YouAre { player },
            });
            // Playtest players never pass class pick — push the dev loadout
            // to the hotbar directly so the weapons are visible/usable.
            let mut slots: Vec<Option<shared::protocol::Item>> =
                vec![None; config::INVENTORY_SLOTS];
            slots[0] = Some(items::pipe_bat());
            if slots.len() > 1 {
                slots[1] = Some(items::scrap_pistol());
            }
            inv_writer.write(ToClients {
                targets: SendTargets::Single(ClientId::Server),
                message: InventoryUpdate { slots },
            });
            let pos = testmap_spawn_pos(0, test.as_deref(), true);
            info!(
                "editor playtest player spawned at ({:.1}, {:.1}, {:.1})",
                pos.x, pos.y, pos.z
            );
        } else {
            for (i, (mut tf, mut net)) in transforms.iter_mut().enumerate() {
                let pos = testmap_spawn_pos(i, test.as_deref(), true);
                tf.translation = pos;
                net.translation = pos;
            }
        }
    } else {
        for e in playtest.iter() {
            commands.entity(e).despawn();
        }
    }
}
