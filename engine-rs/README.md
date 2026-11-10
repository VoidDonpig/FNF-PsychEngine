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
song whose name contains the text. Keys: A S W D or the arrows, Enter, Esc, B for botplay in the song list.

Without ALSA headers (Linux): add `--no-default-features` to build without sound.

## What it does so far

Song list from the week files of the base game and of mods, the play screen (both strum lines, notes and
holds in the stock note skin, timing windows, score, accuracy, misses, health, botplay), results.

Not there yet: characters, stages and cameras, menus, vocals split per side, events, note types other than
Hurt Note, scripts (Lua/HScript), options, editors, videos, tempo-dependent animation.
