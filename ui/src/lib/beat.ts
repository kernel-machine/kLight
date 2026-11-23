// Beat clock for the UI: blinking "on the beat" without a message per beat.
//
// The engine reports the musical beat position about twice a second; between
// reports the UI extrapolates with the BPM. Purely visual: it never feeds
// anything back to the engine.

export interface BeatAnchor {
  /** Beat position reported by the engine. */
  beat: number;
  /** `performance.now()` when it was received. */
  at: number;
  bpm: number;
  beatsPerBar: number;
}

import { beatLamp } from './core';

let anchor: BeatAnchor | null = null;

export function setBeatAnchor(a: BeatAnchor | null) {
  anchor = a;
}

/** Whether the lamp is lit at `now`, and whether this beat starts a bar. */
export function beatFlash(a: BeatAnchor | null, now: number): { on: boolean; down: boolean } {
  if (!a || !(a.bpm > 0) || !Number.isFinite(a.beat)) return { on: false, down: false };
  // Extrapolate the position since the last report; the lamp rule is the core's.
  const beat = a.beat + ((now - a.at) * a.bpm) / 60000;
  return beatLamp(beat, a.bpm, a.beatsPerBar);
}

export const currentBeatFlash = (now = performance.now()) => beatFlash(anchor, now);
