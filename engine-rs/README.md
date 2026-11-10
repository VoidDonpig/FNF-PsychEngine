# Rust engine

A rewrite of the engine in Rust, growing next to the Haxe one until it can replace it.

- `core/` — the rules: chart files, judgement, score, health, finding songs. No window or sound, all unit tested.
- `game/` — the program: window and drawing (macroquad), sound (cpal + lewton), keyboard.

## Run

From a folder that holds the game's `assets` (and `mods`), such as an existing build or this repository:

```bash
cargo run --release --manifest-path engine-rs/Cargo.toml
```

`--base <folder>` points it at another folder, `--bot` plays by itself, `--play <text>` starts the first
song whose name contains the text. Keys: A S W D or the arrows, Enter, Esc; in the song list O opens the
options and B switches botplay. The options are kept in `funkin-rs-settings.json` in that folder.

Without ALSA headers (Linux): add `--no-default-features` to build without sound.

## What it does so far

Song list from the week files of the base game and of mods, the play screen (both strum lines, notes and
holds in the stock note skin, timing windows, score, accuracy, misses, health, botplay), results.
Options: key binds (two per lane), downscroll, middlescroll, ghost tapping, note offset. On Windows key
presses are timed by the raw input thread of `native/psych_input`, not by the frame that sees them.

Not there yet: characters, stages and cameras, menus, vocals split per side, events, note types other than
Hurt Note, scripts (Lua/HScript), the rest of the options, editors, videos, tempo-dependent animation.
