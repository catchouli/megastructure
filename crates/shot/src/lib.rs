//! Screenshot sidecar: everything needed to render a frame again, as JSON.
//!
//! No GPU and no window here. The game fills a [`Shot`] when the player takes a screenshot and
//! writes it next to the PNG; a headless run reads it back and re-renders the same view. See
//! docs/features/screenshots.md.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// Version written into new sidecars. A reader refuses a newer one.
pub const FORMAT_VERSION: u32 = 1;

fn default_near() -> f32 {
    0.1
}

fn default_view() -> String {
    "final".to_string()
}

fn default_shadow_filter() -> String {
    "smooth".to_string()
}

/// Git state of the build that produced a shot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitInfo {
    pub commit: String,
    /// Uncommitted changes (including untracked files) were present.
    pub dirty: bool,
}

impl GitInfo {
    /// Ask git about the repository containing the current directory, or failing that the one
    /// containing the running executable (so a build under `target/` finds its repo).
    /// `None` if git is missing or neither place is in a repository.
    pub fn detect() -> Option<GitInfo> {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        Self::detect_in(Path::new(".")).or_else(|| exe_dir.and_then(|d| Self::detect_in(&d)))
    }

    pub fn detect_in(dir: &Path) -> Option<GitInfo> {
        let run = |args: &[&str]| -> Option<String> {
            let out = Command::new("git").args(args).current_dir(dir).output().ok()?;
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        };
        let commit = run(&["rev-parse", "HEAD"])?.trim().to_string();
        if commit.is_empty() {
            return None;
        }
        let dirty = run(&["status", "--porcelain"]).is_some_and(|s| !s.trim().is_empty());
        Some(GitInfo { commit, dirty })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraState {
    pub position: [f32; 3],
    /// Radians; 0 looks along +Z, positive turns towards +X.
    pub yaw: f32,
    /// Radians, positive looking up.
    pub pitch: f32,
    /// Vertical field of view, radians.
    pub fov_y: f32,
}

/// One preset field: a number, a list of numbers (colours, per-rank values), or `null` for an
/// unset optional such as `sky_tone: none`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PresetValue {
    Scalar(f32),
    List(Vec<f32>),
    Null,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PresetState {
    /// The named preset the values started from (informational: `fields` is complete).
    pub name: String,
    /// Every preset field in effect, by the name `StylePreset::set` takes.
    pub fields: BTreeMap<String, PresetValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SunState {
    pub position: [f32; 3],
    pub radius: f32,
    pub intensity: f32,
}

/// Player state at the moment of the shot. Informational: a replay only renders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MovementState {
    /// `walk` or `fly`.
    pub mode: String,
    pub grounded: bool,
    pub velocity: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shot {
    pub version: u32,
    /// Local time, ISO 8601 with offset.
    pub timestamp: String,
    #[serde(default)]
    pub git: Option<GitInfo>,
    pub scene: String,
    #[serde(default)]
    pub seed: Option<u64>,
    pub camera: CameraState,
    #[serde(default = "default_near")]
    pub near: f32,
    pub width: u32,
    pub height: u32,
    pub preset: PresetState,
    pub sun: SunState,
    #[serde(default = "default_view")]
    pub view: String,
    #[serde(default = "default_shadow_filter")]
    pub shadow_filter: String,
    #[serde(default)]
    pub movement: Option<MovementState>,
    #[serde(default)]
    pub note: String,
}

impl Shot {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a Shot always serialises") + "\n"
    }

    pub fn from_json(text: &str) -> Result<Shot, String> {
        let shot: Shot = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if shot.version > FORMAT_VERSION {
            return Err(format!("sidecar format version {} is newer than this build understands ({FORMAT_VERSION})", shot.version));
        }
        Ok(shot)
    }

    pub fn read(path: &Path) -> Result<Shot, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Self::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json()).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }
}

/// Local time as ISO 8601 with offset, to the second: `2026-11-15T14:03:09+01:00`.
pub fn iso_timestamp() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// Local time as `YYYYMMDD-HHMMSS`, the base of screenshot file names.
pub fn file_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// A base name no `<stem>.png` or `<stem>.json` in `dir` uses: `stamp`, then `stamp-2`, `stamp-3`.
pub fn unique_stem(dir: &Path, stamp: &str) -> String {
    let taken = |s: &str| dir.join(format!("{s}.png")).exists() || dir.join(format!("{s}.json")).exists();
    if !taken(stamp) {
        return stamp.to_string();
    }
    (2u32..).map(|n| format!("{stamp}-{n}")).find(|s| !taken(s)).expect("an unused suffix exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Shot {
        let mut fields = BTreeMap::new();
        fields.insert("fog_start".to_string(), PresetValue::Scalar(25.0));
        fields.insert("ink".to_string(), PresetValue::List(vec![0.07, 0.065, 0.06]));
        fields.insert("sky_tone".to_string(), PresetValue::Null);
        fields.insert("hatch_spacing".to_string(), PresetValue::Scalar(0.1 + 0.2));
        Shot {
            version: FORMAT_VERSION,
            timestamp: "2026-11-15T14:03:09+01:00".into(),
            git: Some(GitInfo { commit: "abc123".into(), dirty: true }),
            scene: "shaft".into(),
            seed: Some(0x5eed_0001),
            camera: CameraState { position: [-0.7043, 423.074, -101.11447], yaw: std::f32::consts::PI, pitch: -0.123_456_79, fov_y: 70.0f32.to_radians() },
            near: 0.1,
            width: 1280,
            height: 720,
            preset: PresetState { name: "nihei_vast".into(), fields },
            sun: SunState { position: [-4.0e10, 1.4e11, -1.1e11], radius: 7.0e8, intensity: 1.0 },
            view: "final".into(),
            shadow_filter: "smooth".into(),
            movement: Some(MovementState { mode: "walk".into(), grounded: true, velocity: [0.0, -1.5e-3, 2.5] }),
            note: String::new(),
        }
    }

    #[test]
    fn round_trip_is_exact() {
        let s = sample();
        assert_eq!(Shot::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn every_f32_bit_pattern_sampled_round_trips() {
        // A cheap xorshift over bit patterns, finite values only.
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        let mut s = sample();
        for _ in 0..200_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let v = f32::from_bits((x >> 16) as u32);
            if !v.is_finite() {
                continue;
            }
            s.camera.yaw = v;
            let r = f32::from_bits(x as u32);
            if !r.is_finite() {
                continue;
            }
            s.sun.radius = r;
            let back = Shot::from_json(&s.to_json()).unwrap();
            assert_eq!(back.camera.yaw.to_bits(), s.camera.yaw.to_bits());
            assert_eq!(back.sun.radius.to_bits(), s.sun.radius.to_bits());
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v["something_new"] = serde_json::json!({"a": 1});
        v["camera"]["roll"] = serde_json::json!(0.5);
        let back = Shot::from_json(&v.to_string()).unwrap();
        assert_eq!(back, sample());
    }

    #[test]
    fn optional_fields_may_be_missing() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        for k in ["git", "seed", "near", "view", "shadow_filter", "movement", "note"] {
            v.as_object_mut().unwrap().remove(k);
        }
        let back = Shot::from_json(&v.to_string()).unwrap();
        assert_eq!((back.git, back.seed, back.movement), (None, None, None));
        assert_eq!((back.near, back.view.as_str(), back.shadow_filter.as_str(), back.note.as_str()), (0.1, "final", "smooth", ""));
    }

    #[test]
    fn missing_required_field_names_it() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v.as_object_mut().unwrap().remove("camera");
        let e = Shot::from_json(&v.to_string()).unwrap_err();
        assert!(e.contains("missing field `camera`"), "{e}");
    }

    #[test]
    fn newer_version_is_refused() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v["version"] = serde_json::json!(FORMAT_VERSION + 1);
        assert!(Shot::from_json(&v.to_string()).unwrap_err().contains("newer"));
    }

    #[test]
    fn unique_stem_adds_suffixes() {
        let dir = std::env::temp_dir().join(format!("mega_shot_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_stem(&dir, "20260101-000000"), "20260101-000000");
        std::fs::write(dir.join("20260101-000000.png"), b"").unwrap();
        assert_eq!(unique_stem(&dir, "20260101-000000"), "20260101-000000-2");
        std::fs::write(dir.join("20260101-000000-2.json"), b"").unwrap();
        assert_eq!(unique_stem(&dir, "20260101-000000"), "20260101-000000-3");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stamps_have_the_documented_shape() {
        let f = file_stamp();
        assert_eq!((f.len(), f.as_bytes()[8]), (15, b'-'));
        assert!(iso_timestamp().contains('T'));
    }
}
