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

## Where it stands against the Haxe engine

Done:

- **Songs**: found through the week files of the base game and of mods, custom difficulties included.
- **Play**: `psych_v1` and older charts, the stock timing windows, score, accuracy, misses, health, holds,
  Hurt Notes, botplay, tempo changes, sections.
- **Stage**: characters from their `characters/*.json` (Sparrow sheets, idle on the beat, sing and miss
  poses), the week 1 backdrop, the camera following the singer and bumping on the beat, health icons and
  bar colours, pause menu, results.
- **Chart events**: Hey!, Set GF Speed, Add Camera Zoom, Play Animation, Camera Follow Pos, Alt Idle
  Animation, Screen Shake, Change Character, Change Scroll Speed, Play Sound, from the chart and from the
  song's `events.json`. Note types: Hurt Note, Alt Animation, Hey!, GF Sing, No Animation, and sections
  marked for alternative poses or for the girlfriend.
- **HUD**: countdown with its sounds, rating and combo pictures, note splashes, hold end caps, time bar,
  miss sounds, the game over scene (fall, music, retry).
- **Sound**: instrumental and vocals as separate tracks (the player's voice drops out on a miss), song
  position from the sound card's sample count.
- **Options**: key binds (two per lane), downscroll, middlescroll, ghost tapping, note offset, botplay.
- **Input**: on Windows key presses are timed by the raw input thread of `native/psych_input`.

Not there yet:

- **Play screen**: the events and note types that belong to one stage or to scripts (Set Property,
  Philly Glow, the weekend 1 fight notes, custom ones), gameplay changers (speed, playback rate, health
  gain/loss, instakill, practice), the backdrops and mechanics of the other ten stages, pixel stages,
  characters made of several sheets or of Animate atlases, other note and splash skins.
- **Menus**: title, main menu, story mode, freeplay as in the Haxe engine (scores, previews), credits,
  mods (enable, order, settings), achievements, the rest of the options (graphics, visuals, note colours,
  gamepad, languages), transitions, loading screen.
- **Saved data**: high scores, week progress, achievements, mod order (`modsList.txt`), translations.
- **Cutscenes**: dialogue, videos.
- **Scripts**: Lua. HScript is Haxe code run against the Haxe engine's own classes, so mods that use it
  cannot run here as they are.
- **Editors**: chart, character, stage, week, menu character, dialogue, note splash.
- **Other**: gamepads, Discord presence, exclusive fullscreen and the flip-model presenter, FPS counter.
