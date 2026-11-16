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
    /// Shift: walk instead of run when walking (`walk` in a script; see `ALWAYS_RUN`), fast
    /// when flying (`fast`). Both script names hold the same key.
    pub fast: bool,
    /// Space: jump when walking, up when flying.
    pub jump: bool,
    /// C or Ctrl held: crouch when walking (`crouch` in a script). Ctrl is also `down` when
    /// flying; C does nothing there.
    pub crouch: bool,
    /// F pressed (one frame in the windowed game; a script holds it for its segment, and only
    /// the first frame counts): toggle between walking and the fly camera.
    pub toggle_fly: bool,
    /// R pressed: return to the last stable ground (walking only).
    pub reset: bool,
    /// Screenshot requested this frame (P or F12 in the windowed game; a script holds it for
    /// its segment and only the first frame counts).
    pub screenshot: bool,
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
        &[
        "forward",
        "back",
        "left",
        "right",
        "up",
        "down",
        "fast",
        "walk",
        "jump",
        "crouch",
        "toggle_fly",
        "reset",
        "screenshot",
        "look_left",
        "look_right",
        "look_up",
        "look_down",
        "idle",
    ];

    /// Set the action called `name` (`idle` sets nothing). Returns false for an unknown name.
    pub fn set_action(&mut self, name: &str) -> bool {
        match name {
            "forward" => self.forward = true,
            "back" => self.back = true,
            "left" => self.left = true,
            "right" => self.right = true,
            "up" => self.up = true,
            "down" => self.down = true,
            "fast" | "walk" => self.fast = true,
            "jump" => self.jump = true,
            "crouch" => self.crouch = true,
            "toggle_fly" => self.toggle_fly = true,
            "reset" => self.reset = true,
            "screenshot" => self.screenshot = true,
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
