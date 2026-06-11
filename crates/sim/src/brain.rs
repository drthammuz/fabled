//! The NPC "brain": a small hierarchy of personality (traits, fixed at
//! birth), emotional state (fast-moving, event-driven), and memories
//! (records with salience that decays over days). Decisions in `npc.rs`
//! read all three layers. An LLM speech layer can later *render* this
//! state into words — it never decides anything.

use bevy::prelude::*;
use serde::Serialize;

use crate::clock::SimClock;
use crate::events::{EventLog, SimEvent};
use crate::npc::{Needs, Npc};
use crate::params;

/// The genome: fixed at spawn, all in 0..1. V5 evolution will breed these.
#[derive(Component, Clone, Copy, Serialize)]
pub struct Traits {
    /// Hunger-rate multiplier (only trait not in 0..1; ~0.85..1.15).
    pub metabolism: f32,
    /// How fast loneliness builds and how rewarding company is.
    pub sociability: f32,
    /// Work ethic: shift attendance and pushing through fatigue.
    pub diligence: f32,
    /// Reluctance to spend: tavern feels affordable later, ale needs a
    /// fuller purse.
    pub frugality: f32,
}

/// Emotional state. One well-modeled emotion beats five vague ones; mood
/// drifts toward its baseline and gets pushed by events and misery.
#[derive(Component)]
pub struct Emotions {
    /// 0 = miserable, 100 = elated, baseline 50.
    pub mood: f32,
}

impl Default for Emotions {
    fn default() -> Self {
        Self { mood: 55.0 }
    }
}

#[derive(Clone, Serialize)]
pub struct Memory {
    pub tick: u64,
    /// Who this memory is about, if anyone.
    pub about: Option<String>,
    pub kind: String,
    /// -1 (awful) .. +1 (wonderful).
    pub valence: f32,
    /// Fades daily; the memory is forgotten below the floor.
    pub salience: f32,
}

#[derive(Component, Default)]
pub struct Memories(pub Vec<Memory>);

impl Memories {
    /// How this NPC feels about `person`, summed over what it remembers.
    pub fn feeling_about(&self, person: &str) -> f32 {
        self.0
            .iter()
            .filter(|memory| memory.about.as_deref() == Some(person))
            .map(|memory| memory.valence * memory.salience)
            .sum()
    }
}

/// Records a memory and logs its formation.
#[allow(clippy::too_many_arguments)]
pub fn remember(
    memories: &mut Memories,
    npc: &str,
    kind: &str,
    about: Option<&str>,
    valence: f32,
    clock: &SimClock,
    log: &mut EventLog,
) {
    memories.0.push(Memory {
        tick: clock.tick,
        about: about.map(str::to_string),
        kind: kind.to_string(),
        valence,
        salience: 1.0,
    });
    if memories.0.len() > params::MEMORY_CAP {
        // Forget the least salient memory to stay within budget.
        let weakest = memories
            .0
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.salience.total_cmp(&b.salience))
            .map(|(index, _)| index)
            .unwrap_or(0);
        memories.0.remove(weakest);
    }
    log.log(
        clock,
        &SimEvent::MemoryFormed {
            npc: npc.to_string(),
            kind: kind.to_string(),
            about: about.map(str::to_string),
            valence,
        },
    );
}

/// Per-tick mood drift: toward baseline, dragged down by misery.
pub fn emotions_tick(mut npcs: Query<(&mut Emotions, &Needs)>) {
    for (mut emotions, needs) in &mut npcs {
        // 3%/hour of the gap back toward baseline.
        let drift = (params::MOOD_BASELINE - emotions.mood) * 0.03 / 60.0;
        emotions.mood += drift;
        if needs.hunger >= 100.0 || needs.warmth <= 0.0 {
            emotions.mood -= params::MOOD_MISERY_PER_HOUR / 60.0;
        }
        if needs.social >= 90.0 {
            emotions.mood -= params::MOOD_LONELY_PER_HOUR / 60.0;
        }
        emotions.mood = emotions.mood.clamp(0.0, 100.0);
    }
}

/// Daily memory decay at the day boundary; weak memories are forgotten.
pub fn decay_memories(clock: Res<SimClock>, mut npcs: Query<&mut Memories, With<Npc>>) {
    if clock.minute_of_day() != 0 {
        return;
    }
    for mut memories in &mut npcs {
        for memory in &mut memories.0 {
            memory.salience *= params::MEMORY_DECAY_PER_DAY;
        }
        memories.0.retain(|memory| memory.salience >= params::MEMORY_MIN_SALIENCE);
    }
}
