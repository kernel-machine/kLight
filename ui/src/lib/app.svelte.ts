// Reactive bridge between the engine client and Svelte components.

import { EngineClient, defaultUrl, CommandFailed, type ClientState, type SocketLike } from './client';
import type { Command } from './protocol';
import { setBeatAnchor } from './beat';

const params = new URLSearchParams(typeof location !== 'undefined' ? location.search : '');

export const client = new EngineClient(
  params.get('engine') ?? defaultUrl(typeof location !== 'undefined' ? location : undefined),
  (url) => new WebSocket(url) as unknown as SocketLike,
  params.get('token'),
);

class AppState {
  /** Immutable snapshot of what the engine published: replaced, never mutated,
   *  so objects that did not change keep their identity across messages. */
  s = $state.raw<ClientState>(client.state);
  notices = $state<{ id: number; text: string; kind: 'error' | 'warning' | 'info' }[]>([]);
}

export const app = new AppState();
let lastStatus: unknown = null;
client.subscribe((s) => {
  app.s = s;
  // A new status carries the beat position: re-anchor the beat lamps.
  if (s.status && s.status !== lastStatus) {
    lastStatus = s.status;
    const tempo = s.runtime?.state.tempo;
    setBeatAnchor(tempo ? { beat: s.status.beat, at: performance.now(), bpm: tempo.bpm, beatsPerBar: tempo.beats_per_bar } : null);
  }
});

let noticeId = 0;
export function notify(text: string, kind: 'error' | 'warning' | 'info' = 'info') {
  const id = ++noticeId;
  app.notices = [...app.notices, { id, text, kind }].slice(-5);
  setTimeout(() => {
    app.notices = app.notices.filter((n) => n.id !== id);
  }, kind === 'error' ? 8000 : 4000);
}

/** Sends a command and reports errors/warnings to the operator. */
export async function run(command: Command): Promise<boolean> {
  try {
    const warnings = await client.command(command);
    for (const w of warnings.slice(0, 3)) notify(w, 'warning');
    return true;
  } catch (e) {
    notify(e instanceof CommandFailed ? e.message : String(e), 'error');
    return false;
  }
}

// ---- Local-only preferences (never influence the output) ----
export function pref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(`klight.${key}`);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

export function setPref(key: string, value: unknown) {
  try {
    localStorage.setItem(`klight.${key}`, JSON.stringify(value));
  } catch {
    /* private mode */
  }
}
