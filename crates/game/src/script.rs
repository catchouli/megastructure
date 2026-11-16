//! Scripted input for headless runs: `forward:2.0,look_right+forward:1.0,idle:0.5`.

use crate::input::InputState;
use anyhow::{anyhow, bail, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub input: InputState,
    pub seconds: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub segments: Vec<Segment>,
}

impl Script {
    /// Parse `action[+action...]:seconds` segments separated by commas.
    pub fn parse(text: &str) -> Result<Script> {
        if text.trim().is_empty() {
            bail!("input script is empty");
        }
        let mut segments = Vec::new();
        for (n, part) in text.split(',').enumerate() {
            let part = part.trim();
            let seg = n + 1;
            if part.is_empty() {
                bail!("input script: segment {seg} is empty");
            }
            let (names, secs) = part
                .split_once(':')
                .ok_or_else(|| anyhow!("input script: segment {seg} '{part}' needs the form action[+action]:seconds"))?;
            let seconds: f32 = secs
                .trim()
                .parse()
                .map_err(|_| anyhow!("input script: segment {seg} '{part}': '{}' is not a number of seconds", secs.trim()))?;
            if !seconds.is_finite() || seconds < 0.0 {
                bail!("input script: segment {seg} '{part}': duration must be a finite number >= 0");
            }
            let mut input = InputState::default();
            let actions: Vec<&str> = names.split('+').map(str::trim).collect();
            if actions.len() > 1 && actions.contains(&"idle") {
                bail!("input script: segment {seg} '{part}': idle cannot be combined with other actions");
            }
            for name in actions {
                if !input.set_action(name) {
                    bail!("input script: segment {seg} '{part}': unknown action '{name}'; available: {}", InputState::ACTION_NAMES.join(", "));
                }
            }
            segments.push(Segment { input, seconds });
        }
        Ok(Script { segments })
    }

    #[cfg(test)]
    pub fn total_seconds(&self) -> f32 {
        self.segments.iter().map(|s| s.seconds).sum()
    }

    /// Number of fixed steps of length `dt` the script covers.
    pub fn total_frames(&self, dt: f32) -> u64 {
        self.boundaries(dt).last().copied().unwrap_or(0)
    }

    /// Cumulative end frame of each segment: the cumulative time divided by `dt`, rounded, so
    /// that whole-second segments at 1/60 s are exactly 60 frames each.
    fn boundaries(&self, dt: f32) -> Vec<u64> {
        let mut cum = 0.0f64;
        self.segments
            .iter()
            .map(|s| {
                cum += s.seconds as f64;
                (cum / dt as f64).round() as u64
            })
            .collect()
    }

    /// The input held during step `frame` (0-based). Idle once the script has ended.
    pub fn input_at(&self, frame: u64, dt: f32) -> InputState {
        for (seg, end) in self.segments.iter().zip(self.boundaries(dt)) {
            if frame < end {
                return seg.input;
            }
        }
        InputState::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example() {
        let s = Script::parse("forward:2.0,look_right:1.0,idle:0.5,back:1.0").unwrap();
        assert_eq!(s.segments.len(), 4);
        assert!(s.segments[0].input.forward && s.segments[0].seconds == 2.0);
        assert!(s.segments[1].input.look_right);
        assert_eq!(s.segments[2].input, InputState::default());
        assert!(s.segments[3].input.back);
        assert!((s.total_seconds() - 4.5).abs() < 1e-6);
    }

    #[test]
    fn combines_and_trims() {
        let s = Script::parse(" forward + look_left : 1.5 ").unwrap();
        assert!(s.segments[0].input.forward && s.segments[0].input.look_left);
    }

    #[test]
    fn every_action_is_accepted() {
        for name in InputState::ACTION_NAMES {
            Script::parse(&format!("{name}:1")).unwrap();
        }
    }

    #[test]
    fn walking_actions_set_their_fields() {
        let s = Script::parse("jump+walk:1,reset:0.1,toggle_fly:0.1,fast:1").unwrap();
        assert!(s.segments[0].input.jump && s.segments[0].input.fast);
        assert!(s.segments[1].input.reset && !s.segments[1].input.fast);
        assert!(s.segments[2].input.toggle_fly);
        assert!(s.segments[3].input.fast && !s.segments[3].input.jump);
        let s = Script::parse("crouch+forward:1").unwrap();
        assert!(s.segments[0].input.crouch && s.segments[0].input.forward);
    }

    #[test]
    fn rejects_bad_scripts() {
        for bad in ["", "forward", "forward:", "forward:x", "forward:-1", "forward:nan", "duck:1", "forward+duck:1", "forward:1,,back:1", "forward:1,", "idle+forward:1", ":1", "forward:inf", "run:1", "duck:1"] {
            assert!(Script::parse(bad).is_err(), "'{bad}' should fail");
        }
        let e = Script::parse("duck:1").unwrap_err().to_string();
        assert!(e.contains("unknown action 'duck'") && e.contains("look_right") && e.contains("toggle_fly"), "{e}");
    }

    #[test]
    fn timeline_is_exact_at_sixty_hertz() {
        let dt = 1.0 / 60.0;
        let s = Script::parse("forward:2.0,idle:0.5,back:1.0").unwrap();
        assert_eq!(s.total_frames(dt), 210);
        assert!(s.input_at(0, dt).forward && s.input_at(119, dt).forward);
        assert_eq!(s.input_at(120, dt), InputState::default());
        assert_eq!(s.input_at(149, dt), InputState::default());
        assert!(s.input_at(150, dt).back && s.input_at(209, dt).back);
        assert_eq!(s.input_at(210, dt), InputState::default());
    }
}
