// What the effect editors (movement, matrix) share: the draft they show, kept
// in step with the engine's version, sent on each change through one queue,
// each edit applied to the latest version of the function.
import { app, notify, run } from './app.svelte';
import { draftSync } from './draftSync';
import { editQueue } from './queue';
import type { Function as Fn } from './protocol';

type Kinds = 'movement_effect' | 'matrix_effect';

/** The effect's fields, without the tag of the function kind. */
export function stripKind<T>(kind: Fn['kind'], type: Kinds): T | null {
  if (kind.type !== type) return null;
  const { type: _type, ...rest } = kind;
  return $state.snapshot(rest) as T;
}

/**
 * `getF` gives the function being edited; `get`/`set` read and replace the
 * editor's draft (a `$state` of the component).
 */
export function fxDraft<T>(type: Kinds, getF: () => Fn, get: () => T | null, set: (d: T | null) => void) {
  const sync = draftSync<T>();
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  const latest = (id: number) => app.s.show?.functions.find((x) => x.id === id);
  const strip = (kind: Fn['kind']) => stripKind<T>(kind, type);
  const snap = (d: T | null) => (d ? ($state.snapshot(d) as T) : null);

  /** The engine's version came in (call it from an `$effect`, inside `untrack`). */
  function follow(incoming: T | null) {
    if (!incoming) return;
    const shown = sync.incoming(getF().id, incoming, snap(get()));
    if (shown) set(shown);
  }

  function commit() {
    const draft = get();
    if (!draft) return;
    const kind = snap(draft) as T;
    sync.sent(kind);
    const id = getF().id;
    enqueue(async () => {
      const current = latest(id);
      if (!current || current.kind.type !== type) return;
      const ok = await run({ type: 'set_function', function: { ...current, kind: { type, ...kind } as unknown as Fn['kind'] } });
      const now = latest(id);
      const engine = now ? strip(now.kind) : null;
      if (!engine || getF().id !== id) return;
      if (ok) {
        const shown = sync.settled(kind, engine, snap(get()));
        if (shown) set(shown);
        return;
      }
      // Refused: back to what the engine has.
      sync.refused();
      set(sync.incoming(id, engine, null));
    });
  }

  /** Renamed on the latest version: an edit not confirmed yet is kept. */
  function rename(name: string) {
    const id = getF().id;
    enqueue(() => {
      const current = latest(id);
      if (!current || current.name === name) return;
      return run({ type: 'set_function', function: { ...current, name } });
    });
  }

  return { strip, follow, commit, rename };
}
