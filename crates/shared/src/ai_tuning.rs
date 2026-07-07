//! Enemy perception tuning — shared so the server AI and the client's
//! dev-tools range indicators can never drift apart.

/// Base vision range (m) along the facing cone.
pub const VISION_RANGE: f32 = 22.0;
/// Vision/hearing multiplier while an enemy's high-alert window is active
/// (Search after contact) — approximated client-side by alerted modes.
pub const ALERT_SENSE_MULT: f32 = 1.35;
/// cos of the vision cone half-angle (55° each side → ~110° FOV).
pub const VISION_CONE_COS: f32 = 0.573_58;
/// Vision cone half-angle in radians (55°).
pub const VISION_CONE_HALF_RAD: f32 = 0.959_93;
/// Crouched players are visible at this fraction of the normal range.
pub const CROUCH_VIS_MULT: f32 = 0.6;
pub const HEAR_SPRINT: f32 = 16.0;
pub const HEAR_WALK: f32 = 9.0;
pub const HEAR_SNEAK: f32 = 3.5;
/// One-shot noise radius of a gunshot.
pub const GUNSHOT_NOISE: f32 = 30.0;

/// Hearing radius for a player moving in the given way. Zero when still —
/// standing players only give themselves away by gunfire.
pub fn hearing_radius(moving: bool, sprint: bool, crouch: bool) -> f32 {
    if !moving {
        0.0
    } else if sprint {
        HEAR_SPRINT
    } else if crouch {
        HEAR_SNEAK
    } else {
        HEAR_WALK
    }
}
