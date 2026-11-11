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
song whose name contains the text, `--story <week file>` a whole week,
`--menu <main|story|freeplay|mods|credits|achievements>` opens at that menu, `--options [page]` at the options. Keys: A S W D or the arrows, Enter or Space, Esc or Backspace. The options are kept in
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
- **Stage mechanics**: lightning over the haunted house (thunder, scared poses, flash), the window lights
  and the passing train of week 3 (with the girlfriend's hair blowing), the car speeding past the limo,
  the tank rolling round the hill of week 7.
- **Chart events**: Hey!, Set GF Speed, Add Camera Zoom, Play Animation, Camera Follow Pos, Alt Idle
  Animation, Screen Shake, Change Character, Change Scroll Speed, Play Sound, from the chart and from the
  song's `events.json`. Note types: Hurt Note, Alt Animation, Hey!, GF Sing, No Animation, and sections
  marked for alternative poses or for the girlfriend.
- **Pixel stages**: notes, holds, ratings, combo digits and countdown in their pixel look and colours,
  the pixel countdown and game over sounds, and the pixel game over character.
- **HUD**: countdown with its sounds, rating and combo pictures, note splashes, hold end caps, time bar,
  miss sounds, the game over scene (fall, music, retry).
- **Sound**: instrumental and vocals as separate tracks (the player's voice drops out on a miss), song
  position from the sound card's sample count.
- **Menus**: the title screen with its opening lines, the main menu, story mode (week list, locked weeks,
  difficulties, the three characters, track list, week score; a week plays through song after song) and
  freeplay (bold song names, icons, song colours, difficulties, personal best), with the menu music and
  sounds. Week order follows `weekList.txt`. Credits (those of the mods first; Enter opens a link) and the
  mods menu: switch mods on and off and move them up and down, kept in `modsList.txt` as the Haxe engine
  keeps it. Mods switched off bring no weeks; mods that run globally lend their files to every song.
- **Achievements**: the Haxe engine's list and rules (a week on Hard without a miss, a rating of 100% or
  under 20%, no idling, two keys only, ten seconds of singing, Friday night), a notice when one is
  unlocked, and the achievements menu.
- **Saved data**: best score and accuracy of every song and difficulty, best week scores, finished weeks
  (which unlock the weeks after them). Runs on botplay are not kept.
- **Options**: key binds (two per lane); gameplay: downscroll, middlescroll, opponent notes, ghost
  tapping, reset button (R), hitsound volume, rating offset, the sick/good/bad hit windows, safe frames,
  note offset; visuals: note splash opacity, hide HUD, time bar (left, elapsed, song name, off),
  flashing lights, camera zooms, score text growing on a hit, health bar opacity, FPS counter, pause
  music, combo stacking, hit timing in ms.
- **Gameplay changers** (an options page, also Ctrl in story mode and freeplay): scroll type and speed,
  playback rate (the pitch follows), health gain and loss, instakill on miss, practice mode, botplay.
  Runs in practice mode or on botplay keep no scores.
- **Input**: on Windows key presses are timed by the raw input thread of `native/psych_input`.

Not there yet:

- **Play screen**: the events and note types that belong to one stage or to scripts (Set Property,
  Philly Glow, the weekend 1 fight notes, custom ones), the rest of what stages do by themselves (the limo's henchmen, the
  running soldiers of Stress, crowd reactions to events, the speaker and lighting of weekend 1), the weekend 1
  fight stage, coarse note splashes on pixel stages, characters made of several sheets or of
  Animate atlases, other note and splash skins.
- **Menus**: the mods menu's mouse buttons, per-mod settings and restart flag, the achievements
  that need features not here yet (the henchmen count, the toaster one, the title easter egg), those of mods, and resetting one, the rest of the options (graphics, note
  colours and skins, delay and combo placement, auto pause, gamepad, languages; the options menu is a plain
  list, not the Haxe engine's look), in freeplay the song preview and score reset,
  transitions between screens, loading screen, a mod's own title screen (`gfDanceTitle.json`).
- **Saved data**: translations. Scores of the Haxe engine are
  not read (it keeps them in its own binary save).
- **Cutscenes**: dialogue, videos.
- **Scripts**: Lua. HScript is Haxe code run against the Haxe engine's own classes, so mods that use it
  cannot run here as they are.
- **Editors**: chart, character, stage, week, menu character, dialogue, note splash.
- **Other**: gamepads, Discord presence, exclusive fullscreen and the flip-model presenter, FPS counter.
