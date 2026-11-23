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
- **FX**: scenes (fades, delays, fades spread over the group), chasers (times
  in ms or beats), cue lists with GO/Back/Pause and *move in black*, movement
  effects and matrix effects, palettes.
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
