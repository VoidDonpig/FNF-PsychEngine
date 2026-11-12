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
- **Input**: on Windows key presses are timed by the raw input thread of `native/psych_input`; the Haxe
  fork's Display Lag Correction (presses judged against the picture on screen, the time frames take to be shown
  taken off).

- **Since**: rating names and full-combo grades in the score line; the pause menu's Change Difficulty and
  Options; "Sustains as One Note" (off: holds judged sixteenth by sixteenth); arrows dropping in at the start;
  a chart's `offset`; a character's own voice file (`vocals_file`); menu and volume keys that can be bound;
  volume keys and display; auto pause; the flashing lights warning at first start; Alt+Enter; memory in the FPS
  counter; note skins and note splash skins; stages of mods built from the `objects` of their stage file; the
  spirit's ghost trail in Thorns; Tankman's lines at week 7's game over; the counted Roadkill Enthusiast
  achievement; a loading picture while a song is read; a crash log (`funkin-rs-crash.log`); a mod's own
  title screen (`images/gfDanceTitle.json`: places, tempo, the dancer's frames, a background), the menus
  looking in the first mod switched on before the game's files; the title screen's four hidden dancers
  (their words typed on it, the jingles, Pessy's achievement); a dark edge closing over the old screen
  before every change of screen, as one uncovers the new; a chart's own note and splash pictures
  (`arrowSkin`, `splashSkin`) and notes left in the colours they were painted in (`disableNoteRGB`);
  sprite sheets described in JSON (Aseprite, TexturePacker) beside the XML and text kinds; note splashes
  as coarse as the rest on pixel stages; the title screen's colours turned with Left and Right; languages
  (`data/<language>.lang` of the game and of mods: the words the Haxe engine translates, and the pictures
  and sounds a language brings in place of the usual ones), chosen at the bottom of the options; the
  stages' own scenes in story mode: lightning before Monster, the lights coming on before Winter Horrorland
  and going out after the song before it; the options' screen for the note delay (its own music, a bar,
  "Beat Hit!" on the beat as it will be judged) and for placing the rating and the combo count;
  Tankman's talks before Ugh, Guns and Stress in story mode (his lines and music, the view, the beep, the
  tears, the girlfriend at gunpoint and Pico's rescue; holding Accept for a second skips them); the rest of
  stage files' `objects`: drawn between the characters where listed (and the characters in the order listed),
  `flipY`, colour names, and objects shown only in story mode or only in freeplay; the mouse where the Haxe
  menus take it (main menu items pointed at and clicked, the wheel in freeplay, story mode and the mods menu,
  a click on a mod, the rating and the combo count dragged on the delay screen), its pointer shown while it moves;
  the pause menu and the options as the Haxe engine draws them (large letters, tick boxes, the description box,
  the list of languages); Discord's "Playing" box (the Haxe engine's texts, the opponent's icon and the time
  left, over Discord's local pipe without a library; the Discord Rich Presence option turns it off); the Haxe
  engine's save taken in at first start (`funkin.sol` in the game's folder, or where the Haxe engine keeps it:
  best scores and accuracy, week totals, finished weeks, achievements, the title's hidden dancer, mods' settings); on the
  streets of weekend 1, Nene's knife (raised while the player's health is at a quarter or less) and her combo
  cheers, the shell off Pico's gun, the faint copy of him as he cocks it, the streets going dark at his shot,
  and his flicker after the can hits him; the fight's two rare turns when the player is nearly out (a badly
  timed hit ducked under for an uppercut, or met with a high punch while one is wound up; the punch that ends it);
  light added as the Haxe engine adds it (its ADD blend): the streets' light maps, Dad Battle's spotlight and
  smoke, the lightning of week 2 and of the Blazin stage; the key binds', note colours' and mods menu's pages
  as in the Haxe engine (rows in large letters with the two keys in boxes; the colour wheel, brightness,
  palette and typed code; the mods menu's buttons, mouse and dragging); weekend 1's scene with the can before
  Darnell in story mode, and its game over (the RETRY sign over Pico, Nene's knife toss); a mod's own settings
  (`data/settings.json`, from the mods menu's settings button; switches, numbers, percentages, lists and keys,
  kept in the settings file by mod); Reset on the options pages puts an option back, and held Left/Right keeps a
  number going; Lua scripts (`scripts/`, `data/<song>/`, `stages/<stage>.lua`), each in a Lua of its own as in
  the Haxe engine, with its globals and callbacks (onCreate, onUpdate, onStepHit, onBeatHit, onSectionHit,
  goodNoteHit, opponentNoteHit, noteMiss, onEvent, onEndSong, onPause, onGameOver, onTimerCompleted...) and a
  first set of functions (debugPrint, timers, sounds, events, health, character poses, setVar/getVar,
  getModSetting, and getProperty/setProperty for the camera, health, score and characters); the scripts' sprites
  (pictures, animated sheets, plain rectangles; behind the characters, in front, on the HUD or over everything;
  their place, size, scroll, angle, colour and animations) and tweens with Flixel's easings; the scripts of
  custom note types and events (`custom_notetypes/`, `custom_events/`, the chart keeping its types' names); keys
  (by the game's actions or Flixel's names) and the mouse; camera flashes; score, misses and hits; the scripts'
  own saves (`funkin-rs-lua-saves/`); files, folders and colours; the strums as scripts move them
  (getPropertyFromGroup/setPropertyFromGroup on strumLineNotes, playerStrums and opponentStrums: place, opacity,
  turn, direction, visibility; noteTweenX/Y/Alpha/Angle/Direction; the default strum places), the notes following
  their strum along its direction; texts (makeLuaText and the setText/getText functions: words, size, width,
  alignment, colour, outline, font); the notes (getPropertyFromGroup/setPropertyFromGroup on unspawnNotes and
  notes: their time, lane, side, type, state; ignoreNote, hitCausesMiss, noAnimation, gfNote, visible, multAlpha).
  getProperty/setProperty (and tweens) on the cameras' and the HUD's pieces (camHUD, camGame, iconP1, iconP2,
  healthBar, scoreTxt, timeBar, timeTxt, botplayTxt: alpha and visible), songSpeed, camFollow,
  isCameraOnForcedPos and the characters' visible. Videos: weekend 1's in story mode (before Darnell, after 2hot
  and Blazin), and Lua's startVideo (as a cutscene the countdown or the end waits for, or over the song);
  mp4 files with H.264 pictures and AAC sound, decoded by OpenH264 (built in) and symphonia; holding Accept for
  a second skips one, the Haxe engine's ring filling meanwhile. onStartCountdown holds the countdown back until
  startCountdown() or the end of a video. The Haxe engine's own dialogue, as scripts start it (startDialogue:
  `data/<song>/<file>.json`, its language's own file first): the speech bubble (normal or angry, at the side or
  in the middle), the portraits of `images/dialogue/<name>.json` sliding in with their expressions, the lines typed
  in the plain letters (wrapping as the Haxe engine's do), music rising under it, Back to skip; onNextDialogue and
  onSkipDialogue. Also: callOnLuas/callOnScripts and setOnLuas/setOnScripts (reaching the other scripts once the
  caller's turn is over), isRunning, getRunningScripts, setHealthBarColors, setTimeBarColors, cameraFade,
  cameraSetTarget, the characters' and the camera's place (getCharacterX/Y, setCharacterX/Y, get/set/add camera
  follow point), getMidpointX/Y and getGraphicMidpointX/Y, mouseReleased, gamepadAnalogX/Y, saveFile and
  deleteFile (inside the mods folder only), and the older luaSprite names.
  Sounds by tag (playSound's tag and loop; stopSound, pauseSound, resumeSound, soundFadeIn/Out/Cancel, the
  volume, place and pitch got and set, luaSoundExists, onSoundFinished) and the song's music's volume and fades
  (musicFadeIn/Out).
  loadGraphic (whole, or cut in a grid of frames for addAnimation), loadFrames, setBlendMode ("add") and
  objectsOverlap for the scripts' sprites.
- **Chart editor** (first part): 7 (Debug Key #1, on the key binds page with Debug Key #2) during a song opens it on
  that song's chart, and 7 on the main menu opens the editors' list (MasterEditorMenu: the mod folder chosen with
  Left/Right; its Chart Editor opens the folder's first song, and the editor's Escape comes back to it): the Haxe editor's grid (the events' column, the player's four, the opponent's four; the
  section playing light, the others dark), the notes and holds, events shown, the icons, the information box;
  Space plays and stops the music, W/S and the wheel move (the wheel by the beat snap), A/D go a section back or on
  (Shift: four), Home/End, R; a click puts a note down or takes one off (Shift picks it), Q/E make the picked note's
  hold shorter or longer, Delete removes it; Left/Right change the beat snap, Z/X the zoom; Ctrl+Z/Y undo and redo;
  Ctrl+S saves over the chart's file, the one before kept as `<file>.bak` (an older chart is saved in the Psych 1.0
  format, its keys where they were); Enter play-tests the chart as it is, and 7 or the song's end come back to it;
  Escape leaves (twice with changes not saved). Ctrl+drag boxes notes and events in, Shift+click adds one to what is
  picked (or takes it out), Ctrl+A picks the section's; Ctrl+C/X copy and cut them, Ctrl+V pastes them at the line,
  Delete takes them off, Q/E change all their holds. Events are put down, taken off and picked in their column, written
  beside the grid. [ and ] change how fast the music plays (Alt: back to normal). The box at the right has the Haxe
  editor's tabs, on controls of the Haxe engine's kind (PsychUI:
  buttons, tick boxes, numbers with - and +, typed text, lists that drop down): Charting (playback rate, mouse
  scroll snap, the player's and the opponent's hitsounds and the metronome, each track's volume and mute), Song (name, vocals, BPM, scroll speed,
  the characters and stage, typed; save), Section (must hit, GF, alt animation, change BPM, beats; copy, paste, clear,
  swap), Note (the picked note's hold and time, the note type for it and new notes, the mods' types too) and Events
  (the event, its description and values for the picked event and new ones, the mods' events too; the entries of
  one moment: before, after, taken off, added). Undo covers all of it. Every two minutes a changed chart is autosaved
  to `backups/<chart>_<date>.bkp` (the ten newest kept); the Song tab's Load Autosave brings one of this chart's back.
- **Character editor**: 8 (Debug Key #2) during a song opens it on the song's opponent (Escape goes back to the song),
  and the editors' list has it too (on bf, in the mod folder chosen). As the Haxe editor: the character on week 1's
  stage beside the silhouettes of where the opponent and the player stand, the cross where the camera looks at it;
  J/K/L/I move the camera, E/Q zoom, R back to normal; W/S change the animation, Space plays it again, A/D step
  through its frames (held still); the arrow keys (held: on and on) and the mouse with its right button move its
  offset (Shift: ten times), Ctrl+C/V/R/Z copy, paste, reset and undo it; F12 hides the silhouettes, F1 shows the
  help. Its animations listed with their offsets, the frame shown, the zoom, the health bar in its colour and its
  icon. The boxes: Ghost (a pose kept behind, how see-through, highlighted), Settings (the characters there are, playable or
  not, reload, the Haxe editor's template), Animations (pick one; name, prefix, indices, framerate, loop; add/update, remove) and Character
  (image, health icon, vocals file, sing duration, scale, flip X, no antialiasing, its place and the camera's, the
  health bar's colour, or the colour most of its icon is; save over its file, the one before kept as `.bak`, or into the mod folder's characters/
  for a new one). Escape asks twice with changes not saved.
- **Week editor**: from the editors' list, on the mod folder's first week (or a new one): the story menu shows the
  week as it is edited, with its picture where the list would have it; the Haxe editor's Week (songs, the three
  characters, background, display name, week name, week file), Other (locked, hidden until unlocked, the week to
  finish first, difficulties) and Freeplay (each song's colour and icon, hidden from freeplay; the week shown as freeplay shows it, Up/Down
  through its songs) tabs; Load Week (the
  folder's weeks), New Week, Save Week (over its file, the one before kept as `.bak`, or into the folder's weeks/;
  the menus read it at once); Escape asks twice with changes not saved.
- **Menu character editor**: from the editors' list: a story menu character's file (`images/menucharacters/`)
  edited where it stands (opponent, boyfriend or girlfriend; the two others faded): image, scale, idle and confirm
  animations, flip X, antialiasing; the arrow keys move its offset (Shift: ten times), Space plays its cheer; the
  folder's menu characters to load; saved over its file (the one before kept as `.bak`).
- **Dialogue editor**: from the editors' list: a dialogue file's lines (the kind startDialogue plays) edited one at a
  time, each shown as the game shows it (Space again): character, speed, angry box, sound, text in the Dialogue Line
  box; W/S the portrait's expression, A/D the lines, O takes one off, P adds one after; the folder's
  `data/<song>/dialogue*.json` to load; saved over its file (the one before kept as `.bak`).
- **Dialogue portrait editor**: from the editors' list: a portrait's file (`images/dialogue/`) edited with the game's
  dialogue showing it (W/S its expressions, Space the line again, H hides the box): left, center or right; image,
  position, scale, no antialiasing; on the Animations tab its expressions (name, looping and resting frames' prefixes;
  add/update, remove), their two frames drawn see-through over each other (H: one alone), WASD and the arrow keys
  moving their offsets (Shift: ten times); J/K/L/I move the view, Q/E zoom it out and in, R puts it back; saved over
  its file (the one before kept as `.bak`).
- **Note splash editor**: from the editors' list: a splash skin's file (`images/noteSplashes/`) edited, its animations
  played over four strums where Psych's splashes go (Space, or a click on a strum): name, prefix, note data, indices,
  frame rate range (add/update, remove); scale, allow RGB, allow pixel; the arrow keys move an animation's offset
  (Shift: ten times), Ctrl+C/V/R copy, paste and reset it; the folder's skins to load; Template starts it empty,
  Convert TXT turns the skin's old `.txt` config into the JSON one; saved over its file (the one before kept as `.bak`).
- **Gamepads** (Windows): any gamepad plays and goes through the menus with Psych's buttons (D-pad and
  A/B/X/Y for the lanes; D-pad or left stick, A or Start, B, Start, Back for the menus); Lua's gamepad functions.

Not there yet:

- **Play screen**: the events and note types that belong to scripts (Set Property beyond the camera, custom
  ones); the rain of weekend 1 (on the streets and in the fight; the Shaders switch turns it off with the title's
  colour shift) is drawn as falling streaks, growing heavier through the song, not with the Haxe engine's shader; the charting mode entries of the pause menu.
- **Menus**: achievements of mods; the note colours' copy and paste buttons (no clipboard here); the key binds' gamepad
  side and debug keys (no editors yet); the GPU Caching switch (nothing to switch here); update check.
- **Cutscenes**: a video's playOnLoad is not kept waiting.
- **Scripts**: the rest of Lua's functions (notes' own textures and health, shaders, groups, and getProperty/setProperty
  beyond the camera, health, score, the characters' place and the scripts' sprites); custom events and note types run by scripts. HScript is Haxe code run against the Haxe
  engine's own classes, so mods that use it cannot run here as they are. Custom shaders.
- **Editors**: the chart editor's File/Edit/View menus and its Data tab, the waveform, the vortex editor, lists to choose characters and stages from, pasted notes following the mouse before they are put down, the autosave's time and count settings, a song
  chosen to chart from the editors' list;
  the week editor's Copy/Paste Color (no system clipboard here); the
  note splash editor's Shader tab; the stage editor.
- **Other**: gamepads on Linux and macOS (gilrs needs libudev there); binding a gamepad's buttons on the key binds
  page (Psych's buttons are used as they are).
