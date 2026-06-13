//! Player health, enemies, melee combat, and light/noise detection.

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use shared::config;
use shared::items;
use shared::protocol::{Enemy, NetTransform, Player, PlayerAlive, PlayerName};

use crate::character::CharacterSystems;
use crate::items::Inventory;
use crate::players::LatestInput;
use crate::run::RunEntity;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                enemy_patrol,
                enemy_aggro,
                player_attacks,
                enemy_damage_players,
            )
                .chain()
                .after(CharacterSystems)
                .run_if(in_state(ClientState::Disconnected)),
        );
    }
}

#[derive(Component)]
pub struct EnemyBrain {
    home: Vec3,
    wander_target: Vec3,
    alert: f32,
    cooldown: f32,
}

impl EnemyBrain {
    pub fn at(position: Vec3) -> Self {
        Self {
            home: position,
            wander_target: position,
            alert: 0.0,
            cooldown: 0.0,
        }
    }
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

fn enemy_patrol(
    time: Res<Time>,
    mut enemies: Query<(&mut Transform, &mut EnemyBrain, &mut NetTransform), With<Enemy>>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut brain, mut net) in &mut enemies {
        brain.cooldown = (brain.cooldown - dt).max(0.0);
        let offset = brain.wander_target - transform.translation;
        if offset.length_squared() < 0.25 || brain.cooldown <= 0.0 {
            let seed = (transform.translation.x * 17.0 + transform.translation.z * 31.0) as u32;
            let r1 = pseudo_rand(seed);
            let r2 = pseudo_rand(seed.wrapping_add(99));
            let angle = r1 * std::f32::consts::TAU;
            let dist = 3.0 + r2 * 4.0;
            brain.wander_target =
                brain.home + Vec3::new(angle.cos() * dist, 0.0, angle.sin() * dist);
            brain.cooldown = 2.0 + r1 * 3.0;
        }
        if brain.alert < 0.5 {
            let step = offset.normalize_or_zero() * 2.0 * dt;
            transform.translation += step;
            net.translation = transform.translation;
        }
    }
}

fn enemy_aggro(
    mut enemies: Query<(&Transform, &mut EnemyBrain), With<Enemy>>,
    players: Query<(&Transform, &LatestInput, &PlayerAlive), With<Player>>,
) {
    for (etransform, mut brain) in &mut enemies {
        let mut nearest = f32::MAX;
        let mut saw = false;
        for (ptransform, input, alive) in &players {
            if !alive.0 {
                continue;
            }
            let dist = etransform.translation.distance(ptransform.translation);
            nearest = nearest.min(dist);
            // Light/noise: sprinting is audible farther; standing still is quieter.
            let hear = if input.0.sprint { 14.0 } else { 8.0 };
            if dist < hear {
                saw = true;
            }
        }
        brain.alert = if saw {
            1.0
        } else {
            (brain.alert - 0.02).max(0.0)
        };
        if brain.alert > 0.5 {
            if let Some((ptransform, _, _)) = players
                .iter()
                .filter(|(_, _, a)| a.0)
                .min_by(|(a, _, _), (b, _, _)| {
                    etransform
                        .translation
                        .distance(a.translation)
                        .partial_cmp(&etransform.translation.distance(b.translation))
                        .unwrap()
                })
            {
                brain.wander_target = ptransform.translation;
            }
        }
        let _ = nearest;
    }
}

fn player_attacks(
    mut commands: Commands,
    spatial: SpatialQuery,
    colliders: Query<&ColliderOf>,
    mut enemies: Query<(Entity, &mut Health, &Transform)>,
    mut players: Query<
        (Entity, &Transform, &mut LatestInput, &Inventory, &PlayerAlive, &PlayerName),
        With<Player>,
    >,
) {
    for (player, transform, mut input, inventory, alive, _) in &mut players {
        if !alive.0 || !input.0.attack {
            continue;
        }
        input.0.attack = false;
        let has_bat = inventory.0.iter().any(|slot| {
            slot.as_ref().is_some_and(items::is_bat)
        });
        if !has_bat {
            continue;
        }
        let eye = transform.translation + Vec3::Y * config::PLAYER_EYE_HEIGHT;
        let dir =
            Dir3::new(look_direction(input.0.yaw, input.0.pitch)).unwrap_or(Dir3::NEG_Z);
        let Some(hit) = spatial.cast_ray(
            eye.adjust_precision(),
            dir,
            2.5,
            true,
            &SpatialQueryFilter::from_excluded_entities([player]),
        ) else {
            continue;
        };
        let body = colliders
            .get(hit.entity)
            .map(|c| c.body)
            .unwrap_or(hit.entity);
        let Ok((enemy, mut health, _)) = enemies.get_mut(body) else {
            continue;
        };
        health.current -= 25.0;
        if health.current <= 0.0 {
            commands.entity(enemy).despawn();
        }
    }
}

fn enemy_damage_players(
    time: Res<Time>,
    run: Query<&shared::run::RunState, With<RunEntity>>,
    mut players: Query<
        (Entity, &Transform, &mut Health, &mut PlayerAlive, &PlayerName),
        With<Player>,
    >,
    enemies: Query<(&Transform, &EnemyBrain), With<Enemy>>,
) {
    if run
        .single()
        .is_ok_and(|r| r.phase == shared::run::RunPhase::RunOver)
    {
        return;
    }
    let dt = time.delta_secs();
    for (_player, transform, mut health, mut alive, name) in &mut players {
        if !alive.0 {
            continue;
        }
        for (etransform, brain) in &enemies {
            if brain.alert < 0.8 {
                continue;
            }
            let dist = transform.translation.distance(etransform.translation);
            if dist < 1.2 {
                health.current -= 30.0 * dt;
            }
        }
        if health.current <= 0.0 && alive.0 {
            alive.0 = false;
            info!("player {} died", name.0);
        }
    }
}

fn look_direction(yaw: f32, pitch: f32) -> Vec3 {
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0) * -Vec3::Z
}

fn pseudo_rand(seed: u32) -> f32 {
    let mut x = seed;
    x ^= x.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 13;
    x ^= x.wrapping_mul(0xc2b2_ae35);
    x ^= x >> 16;
    (x & 0x00ff_ffff) as f32 / 16_777_216.0
}
