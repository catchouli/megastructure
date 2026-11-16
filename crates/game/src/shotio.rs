//! Screenshots and replay: capture the state that defines a frame into a `mega_shot::Shot`,
//! write the PNG and sidecar pair, and put a sidecar's state back into an `App`.
//!
//! `mega_render` has no way to read a preset field by name or the shadow filter, so the preset
//! is read from its `Debug` output (parsed here, with a test that every field in
//! `StylePreset::FIELDS` is found and that the values round-trip through `StylePreset::set`)
//! and the shadow filter is recorded as the default, which the game never changes.

use crate::app::App;
use anyhow::{anyhow, bail, Context, Result};
use mega_render::{camera::NEAR, DebugView, ShadowFilter, StylePreset, Sun};
use mega_shot::{CameraState, GitInfo, MovementState, PresetState, PresetValue, Shot, SunState, FORMAT_VERSION};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where screenshots go unless `--out` says otherwise, under the directory the game started in.
pub const SCREENSHOT_DIR: &str = "screenshots";

/// The scene's generator seed, where it has one.
pub fn scene_seed(scene: &str) -> Option<u64> {
    match scene {
        "shaft" => Some(mega_gen::shaft::DEFAULT_SEED),
        _ => None,
    }
}

/// Every field of `preset` by `set` name, read from its `Debug` text.
pub fn preset_fields(preset: &StylePreset) -> Result<BTreeMap<String, PresetValue>> {
    let text = format!("{preset:?}");
    let body = text
        .strip_prefix("StylePreset {")
        .and_then(|t| t.strip_suffix('}'))
        .ok_or_else(|| anyhow!("unexpected Debug format of StylePreset"))?;
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, c) in body.char_indices() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    let mut out = BTreeMap::new();
    for part in parts.into_iter().map(str::trim).filter(|p| !p.is_empty()) {
        let (name, value) = part.split_once(':').ok_or_else(|| anyhow!("cannot parse preset field '{part}'"))?;
        let (name, value) = (name.trim(), value.trim());
        let num = |v: &str| v.trim().parse::<f32>().map_err(|_| anyhow!("preset field {name}: '{v}' is not a number"));
        let parsed = if value == "None" {
            PresetValue::Null
        } else if let Some(inner) = value.strip_prefix("Some(").and_then(|v| v.strip_suffix(')')) {
            PresetValue::Scalar(num(inner)?)
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            PresetValue::List(inner.split(',').map(num).collect::<Result<_>>()?)
        } else {
            PresetValue::Scalar(num(value)?)
        };
        out.insert(name.to_string(), parsed);
    }
    Ok(out)
}

/// The text `StylePreset::set` takes for a stored value.
fn set_text(v: &PresetValue) -> String {
    match v {
        PresetValue::Scalar(x) => x.to_string(),
        PresetValue::List(l) => l.iter().map(f32::to_string).collect::<Vec<_>>().join(","),
        PresetValue::Null => "none".to_string(),
    }
}

/// Everything that defines the frame `app` would draw now, at `width` x `height`.
pub fn capture_state(app: &App, width: u32, height: u32) -> Result<Shot> {
    let cam = app.camera();
    let sun = app.renderer.sun();
    let walker = app.sim.walker();
    Ok(Shot {
        version: FORMAT_VERSION,
        timestamp: mega_shot::iso_timestamp(),
        git: GitInfo::detect(),
        scene: app.scene_name.to_string(),
        seed: scene_seed(app.scene_name),
        camera: CameraState { position: cam.position.to_array(), yaw: cam.yaw, pitch: cam.pitch, fov_y: cam.fov_y },
        near: NEAR,
        width,
        height,
        preset: PresetState { name: app.preset_name.to_string(), fields: preset_fields(app.renderer.preset())? },
        sun: SunState { position: sun.position.to_array(), radius: sun.radius, intensity: sun.intensity },
        view: app.renderer.view().name().to_string(),
        // No accessor in mega_render; the game never changes it.
        shadow_filter: "smooth".to_string(),
        movement: Some(MovementState {
            mode: app.sim.mode.name().to_string(),
            grounded: walker.is_some_and(|w| w.grounded),
            velocity: walker.map_or([0.0; 3], |w| w.vel.to_array()),
        }),
        note: String::new(),
    })
}

/// Write `<dir>/<stamp>.png` and `.json` for the frame in `pixels` (RGBA8, `width` x `height`),
/// the state taken from `app`. Returns the PNG path.
pub fn save_screenshot(app: &App, pixels: &[u8], width: u32, height: u32, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let shot = capture_state(app, width, height)?;
    let stem = mega_shot::unique_stem(dir, &mega_shot::file_stamp());
    let png = dir.join(format!("{stem}.png"));
    image::save_buffer(&png, pixels, width, height, image::ColorType::Rgba8).with_context(|| format!("cannot write {}", png.display()))?;
    shot.write(&dir.join(format!("{stem}.json"))).map_err(|e| anyhow!(e))?;
    Ok(png)
}

/// Overrides applied on top of a sidecar's state.
#[derive(Default)]
pub struct Overrides {
    /// `(field, value)` pairs for `StylePreset::set`.
    pub set: Vec<(String, String)>,
    pub view: Option<DebugView>,
}

/// Put `shot`'s camera, preset, sun, view and shadow filter into `app`, then the overrides.
/// Warnings (things that cannot be restored) go to stderr.
pub fn apply_state(app: &mut App, shot: &Shot, overrides: &Overrides) -> Result<()> {
    let mut preset = StylePreset::by_name(&shot.preset.name).unwrap_or_else(|| StylePreset::for_scene(&shot.scene));
    for (name, value) in &shot.preset.fields {
        if let Err(e) = preset.set(name, &set_text(value)) {
            eprintln!("megastructure: warning: sidecar preset field not applied: {e}");
        }
    }
    for name in StylePreset::FIELDS {
        if !shot.preset.fields.contains_key(*name) {
            eprintln!("megastructure: warning: sidecar has no value for preset field '{name}'; using the preset default");
        }
    }
    for (name, value) in &overrides.set {
        preset.set(name, value).map_err(|e| anyhow!("--set {name}={value}: {e}"))?;
    }
    app.renderer.set_preset(preset);
    app.renderer.set_sun(Sun { position: shot.sun.position.into(), radius: shot.sun.radius, intensity: shot.sun.intensity });
    let view = match overrides.view {
        Some(v) => v,
        None => DebugView::from_name(&shot.view).ok_or_else(|| anyhow!("sidecar debug view '{}' is unknown; available: {}", shot.view, view_names()))?,
    };
    app.renderer.set_view(view);
    match ShadowFilter::from_name(&shot.shadow_filter) {
        Some(f) => app.renderer.set_shadow_filter(f),
        None => eprintln!("megastructure: warning: unknown shadow filter '{}' in sidecar; using the default", shot.shadow_filter),
    }
    if (shot.near - NEAR).abs() > f32::EPSILON {
        eprintln!("megastructure: warning: sidecar near plane {} differs from this build's {NEAR}; it cannot be set", shot.near);
    }
    let cam = &mut app.sim.camera;
    cam.position = shot.camera.position.into();
    cam.yaw = shot.camera.yaw;
    cam.pitch = shot.camera.pitch;
    cam.fov_y = shot.camera.fov_y;
    Ok(())
}

pub fn view_names() -> String {
    DebugView::ALL.iter().map(|v| v.name()).collect::<Vec<_>>().join(", ")
}

/// Warn if the sidecar came from another commit or a dirty tree, or this build is dirty.
pub fn warn_on_version_mismatch(shot: &Shot) {
    let now = GitInfo::detect();
    match (&shot.git, &now) {
        (None, _) => eprintln!("megastructure: warning: sidecar has no git information; cannot tell whether this build matches"),
        (Some(then), None) => eprintln!("megastructure: warning: cannot determine the current commit (sidecar: {}{})", short(&then.commit), if then.dirty { ", dirty" } else { "" }),
        (Some(then), Some(now)) => {
            if then.commit != now.commit {
                eprintln!("megastructure: warning: sidecar was taken at commit {} but this build is at {}; the replay may differ", short(&then.commit), short(&now.commit));
            }
            if then.dirty {
                eprintln!("megastructure: warning: the working tree was dirty when the screenshot was taken; the replay may differ");
            }
            if now.dirty {
                eprintln!("megastructure: warning: the working tree is dirty now; the replay may differ");
            }
        }
    }
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(10)]
}

/// Check the scene and seed in `shot` against what `by_name` builds.
pub fn check_scene(shot: &Shot) -> Result<()> {
    if mega_gen::scenes::by_name(&shot.scene).is_none() {
        bail!("sidecar scene '{}' is unknown; available: {}", shot.scene, mega_gen::scenes::NAMES.join(", "));
    }
    if shot.seed != scene_seed(&shot.scene) {
        eprintln!("megastructure: warning: sidecar seed {:?} differs from this build's scene seed {:?}; the geometry may differ", shot.seed, scene_seed(&shot.scene));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_field_is_read_and_round_trips() {
        for name in StylePreset::NAMES {
            let p = StylePreset::by_name(name).unwrap();
            let fields = preset_fields(&p).unwrap();
            for f in StylePreset::FIELDS {
                assert!(fields.contains_key(*f), "{name}: field {f} not read from Debug");
            }
            assert_eq!(fields.len(), StylePreset::FIELDS.len(), "{name}: extra fields read");
            // Apply onto a different preset: must reproduce this one exactly.
            let mut q = if *name == "nihei" { StylePreset::nihei_vast() } else { StylePreset::nihei() };
            for (k, v) in &fields {
                q.set(k, &set_text(v)).unwrap();
            }
            assert_eq!(p, q, "{name}");
        }
    }

    #[test]
    fn sky_tone_none_survives() {
        let mut p = StylePreset::nihei();
        p.sky_tone = None;
        assert_eq!(preset_fields(&p).unwrap()["sky_tone"], PresetValue::Null);
    }
}
