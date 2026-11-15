//! Window-independent input state: what the player is asking for during one update step.

/// Held actions plus the mouse movement accumulated since the previous update.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub fast: bool,
    pub look_left: bool,
    pub look_right: bool,
    pub look_up: bool,
    pub look_down: bool,
    /// Mouse movement in pixels (x right, y down). Zero unless mouse look is active.
    pub mouse_delta: (f32, f32),
}

impl InputState {
    /// Action names accepted by the script grammar, in documentation order.
    pub const ACTION_NAMES: &'static [&'static str] =
        &["forward", "back", "left", "right", "up", "down", "fast", "look_left", "look_right", "look_up", "look_down", "idle"];

    /// Set the action called `name` (`idle` sets nothing). Returns false for an unknown name.
    pub fn set_action(&mut self, name: &str) -> bool {
        match name {
            "forward" => self.forward = true,
            "back" => self.back = true,
            "left" => self.left = true,
            "right" => self.right = true,
            "up" => self.up = true,
            "down" => self.down = true,
            "fast" => self.fast = true,
            "look_left" => self.look_left = true,
            "look_right" => self.look_right = true,
            "look_up" => self.look_up = true,
            "look_down" => self.look_down = true,
            "idle" => {}
            _ => return false,
        }
        true
    }
}
