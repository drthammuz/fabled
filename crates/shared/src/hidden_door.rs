//! Hidden-room entrance doors — shared proximity radii and seal geometry.

/// Player/camera within this distance → door opens (visual + collider off).
pub const PROXIMITY_OPEN_M: f32 = 5.0;
/// Hysteresis: stay open until beyond this distance.
pub const PROXIMITY_CLOSE_M: f32 = 6.5;

/// Thin cuboid half-extents for a closed gate-door seal (4 m opening, ~4.2 m tall).
pub fn seal_cuboid_half_extents(yaw: f32) -> (f32, f32, f32) {
    use crate::kenney_catalog::quantize_yaw;
    let yaw = quantize_yaw(yaw);
    const W: f32 = 2.0;
    const H: f32 = 2.1;
    const T: f32 = 0.12;
    // yaw 0 / π → opening spans X; yaw π/2 / 3π/2 → spans Z
    let rem = (yaw / std::f32::consts::FRAC_PI_2).round() as i32 % 2;
    if rem == 0 {
        (W, H, T)
    } else {
        (T, H, W)
    }
}

/// World Y centre for the seal cuboid (floor level + mid-height).
pub fn seal_center_y(floor: i32) -> f32 {
    use crate::level::MOD_H;
    floor as f32 * MOD_H + 2.05
}
