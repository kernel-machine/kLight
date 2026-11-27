// Keeps an editor's draft of a function in step with the engine's version.
//
// The editor shows its draft and sends it on each change; the engine then
// confirms with a new show. A version that comes in while the editor's own
// edits are still on their way is older than the draft: showing it would
// undo them, and an edit made then would send them back. So the draft is
// replaced only by a new function, by edits made elsewhere while nothing is
// pending here (and nothing is being changed without being sent yet, as
// while dragging), or after an edit was refused.

export function draftSync<T>() {
  let id: number | null = null;
  /** JSON of the engine's version the draft last matched. */
  let known = '';
  /** JSON of the last edit sent and not confirmed yet. */
  let pending: string | null = null;
  let refused = false;

  return {
    /** The engine's version came in: what the editor should show, or null to keep its draft. */
    incoming(fid: number, value: T, draft: T | null): T | null {
      const json = JSON.stringify(value);
      if (fid !== id || draft === null || refused) {
        id = fid;
        known = json;
        pending = null;
        refused = false;
        return value;
      }
      if (pending !== null) {
        // Older versions are skipped until the last edit sent comes back.
        if (json === pending) {
          pending = null;
          known = json;
        }
        return null;
      }
      const untouched = JSON.stringify(draft) === known;
      known = json;
      return untouched ? value : null;
    },
    /** The draft was sent to the engine. */
    sent(value: T) {
      pending = JSON.stringify(value);
    },
    /**
     * The engine confirmed the edit `sent` and now has `engine`. Its own echo
     * may not match exactly what was sent (the engine rounds), so the edit
     * stops being pending here too, if it was the last one; returns the
     * engine's version to show if the draft has not changed since.
     */
    settled(sent: T, engine: T, draft: T | null): T | null {
      const json = JSON.stringify(sent);
      if (pending !== json) return null;
      pending = null;
      known = JSON.stringify(engine);
      return draft !== null && JSON.stringify(draft) === json ? engine : null;
    },
    /** An edit was refused: the next version comes in as is. */
    refused() {
      refused = true;
    },
  };
}
