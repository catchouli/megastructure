//! Interactive viewer and headless runner: fly camera over a named scene.
//! See docs/features/player-movement.md and docs/features/headless-mode.md.

mod app;
mod cli;
mod headless;
mod input;
mod script;
mod sim;
mod windowed;

use anyhow::{anyhow, Result};
use cli::Mode;

fn run() -> Result<()> {
    let Some(args) = cli::parse(std::env::args().skip(1))? else { return Ok(()) };
    let scene = mega_gen::scenes::by_name(&args.scene)
        .ok_or_else(|| anyhow!("unknown scene '{}'; available: {}", args.scene, mega_gen::scenes::NAMES.join(", ")))?;
    match &args.mode {
        Mode::Windowed { exit_after_frames } => windowed::run(&scene, *exit_after_frames),
        Mode::Headless(h) => headless::run(h, &scene),
    }
}

fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("megastructure: error: {e:#}");
        std::process::exit(1);
    }
}
