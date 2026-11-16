//! Command-line parsing.

use crate::script::Script;
use crate::sim::MoveMode;
use anyhow::{anyhow, bail, Result};
use crate::shotio::Overrides;
use mega_render::DebugView;
use std::path::PathBuf;

pub const USAGE: &str = "\
megastructure: walk or fly through a scene.

USAGE:
    megastructure [--scene NAME] [--mode walk|fly] [--exit-after-frames N]
    megastructure --headless [--frames N] [--dt S] [--width W] [--height H]
                  [--input SCRIPT] [--out DIR] [--every K] [--scene NAME] [--mode walk|fly]
    megastructure --headless --shot SIDECAR.json [--out DIR] [--set FIELD=VALUE ...] [--view NAME]

OPTIONS:
    --scene <name>          scene to load (default: the first of the scene list)
    --mode <walk|fly>       how to start: walk (first-person, needs a scene with colliders) or
                            fly (the debug camera); default walk where the scene has colliders
    --exit-after-frames <N> windowed: quit after N frames
    --headless              no window: run a fixed-timestep loop and render offscreen
    --frames <N>            headless: number of frames; default the script length, or 1
    --dt <S>                headless: seconds per frame, default 1/60
    --width <W> --height <H>  headless: image size, default 1280x720
    --input <script>        headless: scripted input, e.g. forward:2,look_right+forward:1.5,idle:0.5
                            actions: forward back left right up down fast walk jump toggle_fly
                            reset screenshot look_left look_right look_up look_down idle
                            (screenshot writes screenshots/<time>.png and .json, or into --out)
    --out <dir>             headless: write frame_NNNN.png here
    --every <K>             headless: write every Kth frame (0-based index divisible by K) and
                            the last; default 0 = last frame only
    --shot <sidecar.json>   headless: render the view saved with a screenshot, at its resolution,
                            to <name>.replay.png (next to the sidecar, or in --out)
    --set <field=value>     with --shot: override a preset field (repeatable)
    --view <name>           with --shot: debug view (final id normals depth bands hatching strokes
                            contours lines shadow)
    -h, --help              print this help

CONTROLS (windowed), walking:
    W A S D        move            Space          jump
    Shift          walk (running is the default)
    R              back to the last stable ground
    F              switch between walking and the fly camera, at the current position
    Arrow keys     turn (90 degrees per second)
    Mouse          look while any button is held, or while the cursor is grabbed (Tab)
    P or F12       screenshot to screenshots/ (PNG and a JSON sidecar)
    Escape         quit
CONTROLS (windowed), flying:
    W A S D        move            E or Space     up
    Shift          fast            Q or Ctrl      down
    Scroll         change speed";

pub struct HeadlessArgs {
    pub frames: u64,
    pub dt: f32,
    pub width: u32,
    pub height: u32,
    pub script: Option<Script>,
    pub out: Option<PathBuf>,
    pub every: u64,
}

pub struct ReplayArgs {
    pub shot: PathBuf,
    pub out: Option<PathBuf>,
    pub overrides: Overrides,
}

pub enum Mode {
    Windowed { exit_after_frames: Option<u64> },
    Headless(HeadlessArgs),
    Replay(ReplayArgs),
}

pub struct Args {
    pub scene: String,
    pub mode: Mode,
    /// `--mode`: walk or fly at the start; None picks walking where the scene has colliders.
    pub move_mode: Option<MoveMode>,
}

fn value(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    it.next().ok_or_else(|| anyhow!("{flag} needs a value"))
}

fn number<T: std::str::FromStr>(s: &str, flag: &str) -> Result<T> {
    s.parse().map_err(|_| anyhow!("{flag}: '{s}' is not a valid number"))
}

/// Parse the arguments after the program name. `Ok(None)` means help was printed.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Option<Args>> {
    let mut it = args.into_iter();
    let mut scene = mega_gen::scenes::DEFAULT.to_string();
    let (mut headless, mut exit_after, mut frames, mut dt) = (false, None, None, None);
    let mut move_mode = None;
    let (mut width, mut height, mut script, mut out, mut every) = (None, None, None, None, None);
    let (mut shot, mut sets, mut view): (Option<PathBuf>, Vec<(String, String)>, Option<DebugView>) = (None, Vec::new(), None);
    let mut scene_given = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--scene" => {
                scene = value(&mut it, &a)?;
                scene_given = true;
            }
            "--shot" => shot = Some(PathBuf::from(value(&mut it, &a)?)),
            "--set" => {
                let kv = value(&mut it, &a)?;
                let (k, v) = kv.split_once('=').ok_or_else(|| anyhow!("--set: '{kv}' needs the form field=value"))?;
                sets.push((k.trim().to_string(), v.trim().to_string()));
            }
            "--view" => {
                let name = value(&mut it, &a)?;
                view = Some(DebugView::from_name(&name).ok_or_else(|| anyhow!("--view: '{name}' is not a debug view; available: {}", crate::shotio::view_names()))?);
            }
            "--headless" => headless = true,
            "--mode" => {
                move_mode = Some(match value(&mut it, &a)?.as_str() {
                    "walk" => MoveMode::Walk,
                    "fly" => MoveMode::Fly,
                    other => bail!("--mode: '{other}' is not walk or fly"),
                })
            }
            "--exit-after-frames" => exit_after = Some(number::<u64>(&value(&mut it, &a)?, &a)?),
            "--frames" => frames = Some(number::<u64>(&value(&mut it, &a)?, &a)?),
            "--dt" => dt = Some(number::<f32>(&value(&mut it, &a)?, &a)?),
            "--width" => width = Some(number::<u32>(&value(&mut it, &a)?, &a)?),
            "--height" => height = Some(number::<u32>(&value(&mut it, &a)?, &a)?),
            "--input" => script = Some(Script::parse(&value(&mut it, &a)?)?),
            "--out" => out = Some(PathBuf::from(value(&mut it, &a)?)),
            "--every" => every = Some(number::<u64>(&value(&mut it, &a)?, &a)?),
            other => bail!("unknown argument '{other}'\n\n{USAGE}"),
        }
    }
    if shot.is_none() && (!sets.is_empty() || view.is_some()) {
        bail!("--set and --view only apply with --shot");
    }
    if let Some(shot) = shot {
        if !headless {
            bail!("--shot only applies with --headless");
        }
        let conflicts = [
            ("--frames", frames.is_some()),
            ("--dt", dt.is_some()),
            ("--width", width.is_some()),
            ("--height", height.is_some()),
            ("--input", script.is_some()),
            ("--every", every.is_some()),
            ("--scene", scene_given),
            ("--mode", move_mode.is_some()),
            ("--exit-after-frames", exit_after.is_some()),
        ];
        if let Some((flag, _)) = conflicts.iter().find(|(_, set)| *set) {
            bail!("{flag} cannot be combined with --shot (the sidecar fixes the scene, view and resolution)");
        }
        for (name, _) in &sets {
            if !mega_render::StylePreset::FIELDS.contains(&name.as_str()) {
                bail!("--set: unknown preset field '{name}'");
            }
        }
        return Ok(Some(Args { scene, mode: Mode::Replay(ReplayArgs { shot, out, overrides: Overrides { set: sets, view } }), move_mode }));
    }
    if !headless {
        let headless_only = [
            ("--frames", frames.is_some()),
            ("--dt", dt.is_some()),
            ("--width", width.is_some()),
            ("--height", height.is_some()),
            ("--input", script.is_some()),
            ("--out", out.is_some()),
            ("--every", every.is_some()),
        ];
        if let Some((flag, _)) = headless_only.iter().find(|(_, set)| *set) {
            bail!("{flag} only applies with --headless");
        }
        return Ok(Some(Args { scene, mode: Mode::Windowed { exit_after_frames: exit_after }, move_mode }));
    }
    if exit_after.is_some() {
        bail!("--exit-after-frames is for the windowed game; use --frames with --headless");
    }
    let dt = dt.unwrap_or(1.0 / 60.0);
    if !dt.is_finite() || dt <= 0.0 {
        bail!("--dt must be a positive number of seconds, got {dt}");
    }
    let (width, height) = (width.unwrap_or(1280), height.unwrap_or(720));
    if width == 0 || height == 0 {
        bail!("--width and --height must be at least 1");
    }
    let frames = match (frames, &script) {
        (Some(n), _) => n,
        (None, Some(s)) => s.total_frames(dt).max(1),
        (None, None) => 1,
    };
    if frames == 0 {
        bail!("--frames must be at least 1");
    }
    Ok(Some(Args { scene, mode: Mode::Headless(HeadlessArgs { frames, dt, width, height, script, out, every: every.unwrap_or(0) }), move_mode }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Option<Args>> {
        parse(s.split_whitespace().map(String::from))
    }

    #[test]
    fn headless_defaults_and_script_length() {
        let Mode::Headless(h) = p("--headless --input forward:2,idle:0.5").unwrap().unwrap().mode else { panic!() };
        assert_eq!((h.width, h.height, h.frames, h.every), (1280, 720, 150, 0));
    }

    #[test]
    fn mode_flag_and_new_actions() {
        assert_eq!(p("--mode walk").unwrap().unwrap().move_mode, Some(MoveMode::Walk));
        assert_eq!(p("--headless --mode fly --input jump+walk:1,reset:0.1,toggle_fly:0.1").unwrap().unwrap().move_mode, Some(MoveMode::Fly));
        assert_eq!(p("--headless").unwrap().unwrap().move_mode, None);
    }

    #[test]
    fn replay_options() {
        let Mode::Replay(r) = p("--headless --shot a/b.json --out o --set fog_start=10 --set ink=0,0,0 --view depth").unwrap().unwrap().mode else { panic!() };
        assert_eq!((r.shot, r.out), (PathBuf::from("a/b.json"), Some(PathBuf::from("o"))));
        assert_eq!(r.overrides.set, vec![("fog_start".into(), "10".into()), ("ink".into(), "0,0,0".into())]);
        assert_eq!(r.overrides.view, Some(DebugView::Depth));
        assert!(p("--shot a.json").is_err());
        assert!(p("--headless --shot a.json --scene shaft").is_err());
        assert!(p("--headless --shot a.json --set nope=1").is_err());
        assert!(p("--headless --shot a.json --set fog_start").is_err());
        assert!(p("--headless --set fog_start=1").is_err());
        assert!(p("--headless --shot a.json --view bogus").is_err());
    }

    #[test]
    fn errors() {
        assert!(p("--bogus").is_err());
        assert!(p("--frames 3").is_err());
        assert!(p("--headless --dt 0").is_err());
        assert!(p("--headless --input crouch:1").is_err());
        assert!(p("--mode run").is_err());
        assert!(p("--mode").is_err());
        assert!(p("--headless --frames 0").is_err());
        assert!(p("--headless --exit-after-frames 3").is_err());
    }
}
