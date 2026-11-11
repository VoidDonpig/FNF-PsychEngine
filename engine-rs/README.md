# Rust engine

A rewrite of the engine in Rust, growing next to the Haxe one until it can replace it.

- `core/` — the rules: chart files, judgement, score, health, finding songs. No window or sound, all unit tested.
- `game/` — the program: drawing (wgpu, on Vulkan, DirectX 12 or OpenGL), window and keyboard (winit), sound
  (cpal + lewton).

## Run

From a folder that holds the game's `assets` (and `mods`), such as an existing build or this repository:

```bash
cargo run --release --manifest-path engine-rs/Cargo.toml
```

`--base <folder>` points it at another folder, `--bot` plays by itself, `--play <text>` starts the first
song whose name contains the text, `--story <week file>` a whole week,
`--menu <main|story|freeplay|mods|credits|achievements>` opens at that menu, `--options [page]` at the options. Keys: A S W D or the arrows, Enter or Space, Esc or Backspace. The options are kept in
`funkin-rs-settings.json` in that folder, scores and finished weeks in `funkin-rs-save.json`.

F11 switches fullscreen. `--opengl` draws with OpenGL whatever the Graphics API option says (the way out when
the chosen one shows nothing). When the chosen graphics API is not there and another is used instead, why is
written to `funkin-rs-graphics.log`.

Without ALSA headers (Linux): add `--no-default-features` to build without sound. On Linux the window needs
`libxkbcommon-x11`, and drawing needs Vulkan or EGL.

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
  the tank rolling round the hill of week 7, and in Stress Pico on the speakers shooting by his own
  chart at the soldiers who run in. On the weekend 1 street the traffic light changes and cars drive
  through both ways, stopping at red. Stage events: Kill Henchmen (the pole sweeping the limo's
  dancers off, the limo fetching new ones), the mall crowd cheering on Hey!, BG Freaks Expression, Trigger
  BG Ghouls, Philly Glow (lights out, the glow, sparks and everyone in one colour), Dadbattle Spotlight
  (the light on whoever sings, the smoke), and Set Property for the camera's zoom and speed.
- **Chart events**: Hey!, Set GF Speed, Add Camera Zoom, Play Animation, Camera Follow Pos, Alt Idle
  Animation, Screen Shake, Change Character, Change Scroll Speed, Play Sound, from the chart and from the
  song's `events.json`. Note types: Hurt Note, Alt Animation, Hey!, GF Sing, No Animation, the can and
  gun notes of weekend 1 (poses, sounds, camera; a missed shot costs much health), and sections marked for
  alternative poses or for the girlfriend. Characters may be drawn from several sheets, or the Animate
  way (a folder with `Animation.json` and a sprite map: a timeline of layers placing pieces, symbols
  nested in symbols, in either spelling Animate exports).
- **The fist fight of weekend 1** (Blazin'): the two fighters' moves for every note hit or missed, who
  stands in front, the screen shaking on a hit, the blurred street under a drifting sky, lightning, and
  its game over.
- **The props of weekend 1**: the speaker the girlfriend sits on (its bars follow seven bands of the
  music, its eyes whoever sings, its body thumps on loud moments), and the spray can: kicked up, then
  shot out of the air or bursting on the player.
- **Pixel stages**: notes, holds, ratings, combo digits and countdown in their pixel look and colours,
  the pixel countdown and game over sounds, and the pixel game over character.
- **Dialogue**: the talks before Senpai, Roses and Thorns in story mode: the text boxes, portraits,
  typed lines, their sounds and music, and Senpai's end before Thorns.
- **HUD**: countdown with its sounds, rating and combo pictures, note splashes, hold end caps, time bar,
  miss sounds, the game over scene (fall, music, retry).
- **Sound**: instrumental and vocals as separate tracks (the player's voice drops out on a miss), song
  position from the sound card's sample count, evened out so that the notes move steadily.
- **Menus**: the title screen with its opening lines, the main menu, story mode (week list, locked weeks,
  difficulties, the three characters, track list, week score; a week plays through song after song) and
  freeplay (bold song names, icons, song colours, difficulties, personal best), with the menu music and
  sounds. Week order follows `weekList.txt`. In freeplay Space opens the song player (pause, wind with
  Left and Right, speed with Up and Down, R to start over), R asks whether to reset the song's score (in
  story mode: the week's), and holding Up or Down runs through the list. Credits (those of the mods first; Enter opens a link) and the
  mods menu: switch mods on and off (one or all), move them up, down or to the top, kept in
  `modsList.txt` as the Haxe engine keeps it; a mod whose `pack.json` asks for a restart sends the game
  back to its title screen. Mods switched off bring no weeks; mods that run globally lend their files to every song.
- **Achievements**: the Haxe engine's list and rules (a week on Hard without a miss, a rating of 100% or
  under 20%, no idling, two keys only, ten seconds of singing, Friday night, the lowest graphics), a
  notice when one is unlocked, and the achievements menu (R locks one again).
- **Saved data**: best score and accuracy of every song and difficulty, best week scores, finished weeks
  (which unlock the weeks after them). Runs on botplay are not kept.
- **Options**: key binds (two per lane); gameplay: downscroll, middlescroll, opponent notes, ghost
  tapping, reset button (R), hitsound volume, rating offset, the sick/good/bad hit windows, safe frames,
  note offset; visuals: note splash opacity, hide HUD, time bar (left, elapsed, song name, off),
  flashing lights, camera zooms, score text growing on a hit, health bar opacity, FPS counter, pause
  music, combo stacking, hit timing in ms, where the rating and the combo count pop up; graphics: low
  quality (the stage details the Haxe engine leaves out are left out), anti-aliasing, frame rate,
  fullscreen; note colours:
  body, shine and outline of each lane, for pixel stages too (turn the hue, or type a colour code).
- **Gameplay changers** (an options page, also Ctrl in story mode and freeplay): scroll type and speed,
  playback rate (the pitch follows), health gain and loss, instakill on miss, practice mode, botplay.
  Runs in practice mode or on botplay keep no scores.
- **Graphics API** (an option, applied at the next start): what the game is drawn with. DirectX 12
  (Windows, where it is the one chosen at first), Vulkan (chosen at first elsewhere) or OpenGL. One the
  machine does not have is replaced by one it has. Frames are shown as soon as they are done; the
  Framerate option limits them. Fullscreen is a window without borders over the screen, or, with the
  Exclusive Fullscreen option, the display taken for the game alone.
- **Input**: on Windows key presses are timed by the raw input thread of `native/psych_input`.

- **Since**: rating names and full-combo grades in the score line; the pause menu's Change Difficulty and
  Options; "Sustains as One Note" (off: holds judged sixteenth by sixteenth); arrows dropping in at the start;
  a chart's `offset`; a character's own voice file (`vocals_file`); menu and volume keys that can be bound;
  volume keys and display; auto pause; the flashing lights warning at first start; Alt+Enter; memory in the FPS
  counter; note skins and note splash skins; stages of mods built from the `objects` of their stage file; the
  spirit's ghost trail in Thorns; Tankman's lines at week 7's game over; the counted Roadkill Enthusiast
  achievement; a loading picture while a song is read; a crash log (`funkin-rs-crash.log`).

Not there yet:

- **Play screen**: the events and note types that belong to scripts (Set Property beyond the camera, custom
  ones); a chart's own `arrowSkin`/`splashSkin` and `disableNoteRGB`; what is left of weekend 1 (Nene's
  knife and cheers, the shell flying off the gun, the stage going dark at the shot, rain, light maps, the two
  rare turns of the fight when the player is nearly out); coarse note splashes on pixel stages; the Haxe
  fork's Display Lag Correction; the charting mode entries of the pause menu.
- **Stages**: objects of a stage file listed between two characters are drawn behind all three; `flipY`,
  colour names and the story-mode/freeplay filters of stage objects.
- **Menus**: the title screen's easter eggs and a mod's own title screen (`gfDanceTitle.json`); the screen
  that closes over the old screen before a change (the new one is uncovered by a wipe); the mods menu's
  mouse buttons and per-mod settings; achievements of mods; the options menu's look (it is a plain list)
  and its Shaders and GPU Caching switches (nothing to switch here); the screen for adjusting delay and
  placing the rating by dragging; Discord presence; update check; languages; the mouse anywhere.
- **Saved data**: scores of the Haxe engine are not read (it keeps them in its own binary save).
- **Cutscenes**: week 2's Monster, week 5's Winter Horrorland and Eggnog, week 7's three, weekend 1's;
  videos; dialogue from a song's `dialogue.json` (the kind scripts start).
- **Scripts**: Lua (about 225 functions and the callbacks). HScript is Haxe code run against the Haxe
  engine's own classes, so mods that use it cannot run here as they are. Custom shaders.
- **Files**: Aseprite (JSON) sprite sheets.
- **Editors**: chart, character, stage, week, menu character, dialogue, dialogue character, note splash.
- **Other**: gamepads.
