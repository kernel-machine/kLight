# kLight — user guide

## Starting

Start the kLight desktop app (or `klight-supervisor` from a terminal). The
engine runs in a separate process: if the interface closes or freezes, the
lights keep going. If the engine itself hangs, the supervisor restarts it
within a few seconds, restoring active cues, levels and mode.

From a tablet or phone open `http://<pc-address>:9400/?token=<token>`.

## Screens

- **Fixtures**: universes, patch from `.qxf` definitions (the same ones used by
  QLC+), automatic next free address, conflict warnings, head orientation
  (inverted or swapped pan/tilt, upside-down mounting), groups with order and grid.
- **FX**: scenes (fades, delays, delays spread over the group, own times per
  attribute), chasers (times in ms or beats), cue lists with GO/Back/Pause and
  *move in black*, movement effects and matrix effects, palettes.
  The scene editor follows the work: *Lights* on the left (the × next to a
  group or a fixture takes it out; each light shows its color and intensity in
  the scene; its bulb lights it at full to find it on stage), *Set* on the right:
  choose an attribute and move its control — the value goes into the scene as
  soon as you let go (Undo takes it back). Below, the scene as *Values* or as
  *DMX channels*; at the bottom the *Timing*, folded into one line. Values
  apply in order: on every channel the last one wins, whoever it goes to; one
  that later ones override entirely is greyed out as "no effect". *Start* runs
  the scene with its own times and pauses the live preview; Duplicate, the
  programmer and Delete are in the ⋯ menu. Pan and tilt can be dragged on an XY
  pad (*Fine*, or Shift, moves ten times slower).
  The chaser editor has the same header: ◀ / ▶ step back and on, Start/Stop,
  and a live preview of the step you pick while it is stopped. The timeline
  shows every step as long as it lasts; while the chaser runs, the current one
  is marked and a click on any step jumps there. Each step can have its own
  *Hold* and *Fade* ("= 1 beat" means it follows the chaser); ⧉ duplicates a
  step; drag the ⠿ handle, or focus it and use ↑/↓, to reorder.
  The movement editor has Start/Stop and the ⋯ menu too. Draw the shape on the
  preview: drag its center to move it and the handles on its right and top
  edges to resize it (or focus them and use the arrow keys, Shift for 10°).
  The shaded area is out of reach of the group's lights; if part of the shape
  goes there, a warning says so, because there the heads stop at their limit.
  The heads are numbered in group order, as in the list below the preview;
  *Together*, *Wave* and *Even* share the cycle among them (phase spread 0°,
  180° and 360°). *Rotation* turns the shape; *Mirrored* makes every second
  head, or the second half of the group, draw it mirrored left to right (⇄ in
  the list, dashed on the preview). *Mode*: loop, single shot (one cycle, then
  it stops) or ping-pong (forward, then back). *Fade in* grows the shape from
  its center when it starts, *Fade out* shrinks it back when it is stopped.
  *Relative to the position of the scenes* adds the shape to where the scenes
  put each head: the center becomes an offset from there.
  The cue list editor runs the show from its header: GO, Back, Pause and
  Stop; above the cues it shows the cue playing, how far its fade is, and the
  next one (with the time left when it follows by itself). Set the lights in
  the Simple Desk, then *Record cue*: the new cue goes after the one playing
  (or at the end), numbered between its neighbours (1.1 between 1 and 2).
  Every cue's number, name, fade in, fade out and delay are edited in its row
  (a bare number is in seconds; "2 beats" follows the tempo), as are *Follow*
  (wait for GO, or go on after a time) and *Move in black* (as the list, on or
  off). Its values are shown on demand; *Update* adds the programmer to a cue
  (the same values are replaced). ⧉ duplicates a cue, ↑/↓ move it, ▶ jumps to
  it; *Renumber* in the ⋯ menu numbers them 1, 2, 3 again.
  The matrix editor has Start/Stop and the ⋯ menu too. Its heads are numbered
  in group order on the grid. Besides solid, chase, wave, gradient and text,
  *Fill* fills the grid one line at a time and *Twinkle* lights a share of the
  cells at random. Chase, wave, gradient and fill move across the columns or
  across the rows; *Mode* is loop, single shot or ping-pong, and *Fade in* /
  *Fade out* bring the level up when it starts and down when it is stopped.
- **Virtual Console**: in Design mode add and move widgets; in Show mode the
  widgets are live and the structure is locked.
- **Simple Desk**: tap a group or some fixtures, then an intention (intensity,
  color, position in degrees, fan, gobo, strobe in Hz). Save as a scene or
  update a palette.
- **Devices**: assign each universe its interface (recognised by USB serial
  number), configure MIDI/OSC controllers and use *MIDI learn*.

Blackout and grand master are always at the top; the DMX monitor can be opened
from any screen. The bottom bar shows output status, controllers, jitter, last
save, connected clients, mode and BPM.

## DMX interfaces

- **Enttec DMX USB Pro**: recommended for live use, it repeats the last frame by itself.
- **Enttec Open DMX**: the PC generates the signal; if the engine stops, the
  output stops too ("no hardware buffer" badge).

## Saving

The show saves itself after every change, atomically, keeping the last 10
backups in the `<show>.backups/` folder. If the file is damaged, the newest
valid backup is loaded.
