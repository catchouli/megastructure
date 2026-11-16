//! Interactive viewer and headless runner: first-person walking (or the fly camera) in a named scene.
//! See docs/features/player-movement.md and docs/features/headless-mode.md.

mod app;
mod cli;
mod headless;
mod input;
mod offscreen;
mod player;
mod script;
mod shotio;
mod sim;
mod walker;
mod windowed;
mod world;

#[cfg(test)]
mod shaft_tests;
#[cfg(test)]
mod vent_tests;

use anyhow::{anyhow, bail, Result};
use cli::Mode;

fn run() -> Result<()> {
    let Some(args) = cli::parse(std::env::args().skip(1))? else { return Ok(()) };
    if let Mode::Replay(r) = &args.mode {
        return headless::replay(r);
    }
    let scene = mega_gen::scenes::by_name(&args.scene)
        .ok_or_else(|| anyhow!("unknown scene '{}'; available: {}", args.scene, mega_gen::scenes::NAMES.join(", ")))?;
    if args.move_mode == Some(sim::MoveMode::Walk) && scene.colliders.is_empty() {
        bail!("--mode walk: scene '{}' has no colliders to walk on; use --mode fly", scene.name);
    }
    match &args.mode {
        Mode::Windowed { exit_after_frames } => windowed::run(&scene, *exit_after_frames, args.move_mode),
        Mode::Headless(h) => headless::run(h, &scene, args.move_mode),
        Mode::Replay(_) => unreachable!("handled above"),
    }
}

fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("megastructure: error: {e:#}");
        std::process::exit(1);
    }
}
