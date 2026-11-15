# Megastructure docs

| File | What it is |
|---|---|
| `PLAN.md` | The overall plan: decisions, frame layout, generation layers, milestones |
| `features/*.md` | One document per feature: its design and technical details |
| `experiments.md` | Log of things we tried and what happened |

## Feature documents

One file per feature, written when work on the feature starts and kept current as it changes. Each has the same sections:

1. **Status**: designed, in progress, working, or parked, with the date.
2. **What it is**: one paragraph.
3. **Design**: behaviour and the reasons for it.
4. **Technical details**: data structures, passes, formats, constants, file locations once code exists.
5. **Open questions**.
6. **Related experiments**: ids from `experiments.md`.

Constants that other systems depend on (for example, movement limits the generator relies on) are listed in a table and marked as contracts.

| Feature | File | Status |
|---|---|---|
| Player movement | `features/player-movement.md` | Designed |
| Sun and sky | `features/sun-and-sky.md` | Designed |
| Machinery | `features/machinery.md` | Designed in outline |
| Capture tool | `features/capture-tool.md` | Working |
| Headless mode | `features/headless-mode.md` | Working |
| Ink renderer | `features/ink-renderer.md` | In progress (M0 frame working) |
| Kit of parts | `features/kit-of-parts.md` | In progress (M1 kit) |
| One Shaft | `features/one-shaft.md` | Working (M1 scene) |

Still to be split out of `PLAN.md` when work starts: space graph, layout, decay, streaming and LOD.

## Testing without a window

All verification is headless. `mega_capture` renders named camera paths offscreen to PNG files, and the game binary has a `--headless` mode that runs its update loop and renderer offscreen for a fixed number of frames, optionally driven by a scripted input (`features/headless-mode.md`). Automated checks and agents never open a window. The windowed game is for a person to run by hand:

```
cargo run --release --bin megastructure
```

## Experiment log

Add an entry for anything tried whose outcome was not known in advance, whether or not it worked. Entries are numbered `E001`, `E002` and so on, newest at the bottom, and are not rewritten afterwards; a follow-up gets a new entry that refers to the old one.
