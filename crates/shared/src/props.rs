//! Dynamic physics props. The component describes *what* a prop is; the
//! server gives it physics, the client gives it a mesh.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PropShape {
    /// Box with full extents `size`.
    Crate { size: Vec3 },
    Ball { radius: f32 },
}

/// Dynamic props the server allows players to grab and throw.
#[derive(Component, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Grabbable;
