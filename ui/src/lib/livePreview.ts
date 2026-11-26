// Keeps the engine showing what an editor is trying out. The engine drops a
// preview that is not renewed, so it is sent again while it lasts; quick
// changes (a dragged control) are sent at most once per interval.
import type { Command, SceneValue, Target } from './protocol';

/** Renewal, well within the time the engine holds a preview. */
export const PREVIEW_EVERY_MS = 1000;
/** Shortest time between two changes sent. */
export const PREVIEW_PACE_MS = 50;

export function livePreview(send: (command: Command) => void) {
  let current: Command | null = null;
  let sent = '';
  let pace: ReturnType<typeof setTimeout> | undefined;
  let renewal: ReturnType<typeof setInterval> | undefined;

  function push() {
    if (!current) return;
    const text = JSON.stringify(current);
    if (text === sent) return;
    sent = text;
    send(current);
    pace = setTimeout(() => {
      pace = undefined;
      push(); // what changed in the meantime
    }, PREVIEW_PACE_MS);
  }

  return {
    /** Shows these scene values on the lights, until changed or stopped. */
    show(values: SceneValue[], members: Target[]) {
      current = { type: 'set_preview', values, members };
      renewal ??= setInterval(() => current && send(current), PREVIEW_EVERY_MS);
      if (pace === undefined) push();
    },
    /** Gives the lights back. */
    stop() {
      if (!current) return;
      current = null;
      sent = '';
      clearTimeout(pace);
      clearInterval(renewal);
      pace = renewal = undefined;
      send({ type: 'clear_preview' });
    },
  };
}
