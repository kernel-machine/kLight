import { describe, expect, it } from 'vitest';
import { draftSync } from './draftSync';

type V = { a: number; b: number };

describe('draftSync', () => {
  it('shows the engine version of a newly opened function', () => {
    const s = draftSync<V>();
    expect(s.incoming(1, { a: 1, b: 1 }, null)).toEqual({ a: 1, b: 1 });
  });

  it('follows edits made elsewhere when nothing is pending here', () => {
    const s = draftSync<V>();
    const shown = s.incoming(1, { a: 1, b: 1 }, null)!;
    expect(s.incoming(1, { a: 5, b: 1 }, shown)).toEqual({ a: 5, b: 1 });
  });

  it('keeps a change not sent yet (a control being dragged)', () => {
    const s = draftSync<V>();
    s.incoming(1, { a: 1, b: 1 }, null);
    expect(s.incoming(1, { a: 1, b: 1 }, { a: 3, b: 1 })).toBeNull();
  });

  // Regression: the confirmation of the first of two quick edits replaced
  // the draft, so an edit made before the second confirmation started from
  // it and sent the first edit's version, losing the second.
  it('does not go back to an older version while its own edits are pending', () => {
    const s = draftSync<V>();
    s.incoming(1, { a: 0, b: 0 }, null);
    s.sent({ a: 1, b: 0 }); // first edit
    s.sent({ a: 1, b: 2 }); // second edit, quickly after
    // The first one is confirmed: the draft already has both.
    expect(s.incoming(1, { a: 1, b: 0 }, { a: 1, b: 2 })).toBeNull();
    // Then the second.
    expect(s.incoming(1, { a: 1, b: 2 }, { a: 1, b: 2 })).toBeNull();
    // Caught up: edits made elsewhere are followed again.
    expect(s.incoming(1, { a: 9, b: 2 }, { a: 1, b: 2 })).toEqual({ a: 9, b: 2 });
  });

  it('starts over when another function is opened', () => {
    const s = draftSync<V>();
    s.incoming(1, { a: 0, b: 0 }, null);
    s.sent({ a: 1, b: 0 });
    expect(s.incoming(2, { a: 7, b: 7 }, { a: 1, b: 0 })).toEqual({ a: 7, b: 7 });
  });

  it('after a refused edit, shows the engine version again', () => {
    const s = draftSync<V>();
    s.incoming(1, { a: 0, b: 0 }, null);
    s.sent({ a: 1, b: 0 });
    s.refused();
    expect(s.incoming(1, { a: 0, b: 0 }, { a: 1, b: 0 })).toEqual({ a: 0, b: 0 });
  });

  it('an edit the engine rounds does not keep it waiting', () => {
    const s = draftSync<V>();
    s.incoming(1, { a: 0, b: 0 }, null);
    s.sent({ a: 0.1, b: 0 });
    // The echo differs from what was sent: still pending.
    expect(s.incoming(1, { a: 0.1000001, b: 0 }, { a: 0.1, b: 0 })).toBeNull();
    // Once confirmed, the engine's version is shown.
    expect(s.settled({ a: 0.1, b: 0 }, { a: 0.1000001, b: 0 }, { a: 0.1, b: 0 })).toEqual({ a: 0.1000001, b: 0 });
    // And an edit made elsewhere afterwards is followed.
    expect(s.incoming(1, { a: 0.1000001, b: 4 }, { a: 0.1000001, b: 0 })).toEqual({ a: 0.1000001, b: 4 });
  });

  it('a confirmation of an older edit does not end the wait for a newer one', () => {
    const s = draftSync<V>();
    s.incoming(1, { a: 0, b: 0 }, null);
    s.sent({ a: 1, b: 0 });
    s.sent({ a: 1, b: 2 });
    expect(s.settled({ a: 1, b: 0 }, { a: 1, b: 0 }, { a: 1, b: 2 })).toBeNull();
    expect(s.incoming(1, { a: 1, b: 0 }, { a: 1, b: 2 })).toBeNull();
  });
});
