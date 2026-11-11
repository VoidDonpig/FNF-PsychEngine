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
song whose name contains the text, `--story <week file>` a whole week, `--menu <main|story|freeplay>` opens
at that menu. Keys: A S W D or the arrows, Enter or Space, Esc or Backspace. The options are kept in
`funkin-rs-settings.json` in that folder, scores and finished weeks in `funkin-rs-save.json`.

Without ALSA headers (Linux): add `--no-default-features` to build without sound.

## Where it stands against the Haxe engine

Done:

- **Songs**: found through the week files of the base game and of mods, custom difficulties included.
- **Play**: `psych_v1` and older charts, the stock timing windows, score, accuracy, misses, health, holds,
  Hurt Notes, botplay, tempo changes, sections.
- **Stage**: characters from their `characters/*.json` (Sparrow sheets, idle on the beat, sing and miss
  poses, frames stored rotated, pixel art), the camera following the singer and bumping on the beat,
  health icons and bar colours, pause menu, results.
- **Backdrops**: the pictures of ten stages (week 1 to week 7 and the weekend 1 streets), with their
  looping and on-the-beat animations and foreground layers. Base game songs without a stage in their chart
  get their usual one.
- **Chart events**: Hey!, Set GF Speed, Add Camera Zoom, Play Animation, Camera Follow Pos, Alt Idle
  Animation, Screen Shake, Change Character, Change Scroll Speed, Play Sound, from the chart and from the
  song's `events.json`. Note types: Hurt Note, Alt Animation, Hey!, GF Sing, No Animation, and sections
  marked for alternative poses or for the girlfriend.
- **HUD**: countdown with its sounds, rating and combo pictures, note splashes, hold end caps, time bar,
  miss sounds, the game over scene (fall, music, retry).
- **Sound**: instrumental and vocals as separate tracks (the player's voice drops out on a miss), song
  position from the sound card's sample count.
- **Menus**: the title screen with its opening lines, the main menu, story mode (week list, locked weeks,
  difficulties, the three characters, track list, week score; a week plays through song after song) and
  freeplay (bold song names, icons, song colours, difficulties, personal best), with the menu music and
  sounds. Week order follows `weekList.txt`.
- **Saved data**: best score and accuracy of every song and difficulty, best week scores, finished weeks
  (which unlock the weeks after them). Runs on botplay are not kept.
- **Options**: key binds (two per lane), downscroll, middlescroll, ghost tapping, note offset, botplay.
- **Input**: on Windows key presses are timed by the raw input thread of `native/psych_input`.

Not there yet:

- **Play screen**: the events and note types that belong to one stage or to scripts (Set Property,
  Philly Glow, the weekend 1 fight notes, custom ones), gameplay changers (speed, playback rate, health
  gain/loss, instakill, practice), what moves by its own rules on the stages (train, cars, lightning,
  window lights, rolling tank, running soldiers, crowd reactions, the speaker of weekend 1), the weekend 1
  fight stage, the pixel look of notes and HUD on pixel stages, characters made of several sheets or of
  Animate atlases, other note and splash skins.
- **Menus**: credits, mods (enable, order, settings), achievements, the rest of the options (graphics,
  visuals, note colours, gamepad, languages), in freeplay the song preview, score reset and gameplay
  changers, transitions between screens, loading screen, a mod's own title screen (`gfDanceTitle.json`).
- **Saved data**: achievements, mod order (`modsList.txt`), translations. Scores of the Haxe engine are
  not read (it keeps them in its own binary save).
- **Cutscenes**: dialogue, videos.
- **Scripts**: Lua. HScript is Haxe code run against the Haxe engine's own classes, so mods that use it
  cannot run here as they are.
- **Editors**: chart, character, stage, week, menu character, dialogue, note splash.
- **Other**: gamepads, Discord presence, exclusive fullscreen and the flip-model presenter, FPS counter.
