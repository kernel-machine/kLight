import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { livePreview, PREVIEW_EVERY_MS, PREVIEW_PACE_MS } from './livePreview';
import type { Command, SceneValue } from './protocol';

const level = (n: number): SceneValue[] => [{ target: { type: 'scene' }, value: { type: 'explicit', value: { type: 'intensity', level: n } } }];

function setup() {
  const sent: Command[] = [];
  const preview = livePreview((c) => sent.push(c));
  const levels = () =>
    sent.map((c) => (c.type === 'set_preview' && c.values[0]?.value.type === 'explicit' && c.values[0].value.value.type === 'intensity' ? c.values[0].value.value.level : c.type));
  return { preview, levels };
}

describe('live preview', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('sends the first change at once, then the latest at its pace', () => {
    const { preview, levels } = setup();
    preview.show(level(0.1), []);
    preview.show(level(0.2), []);
    preview.show(level(0.3), []);
    expect(levels()).toEqual([0.1]);
    vi.advanceTimersByTime(PREVIEW_PACE_MS);
    expect(levels()).toEqual([0.1, 0.3]);
    // The same thing again is not a change.
    vi.advanceTimersByTime(PREVIEW_PACE_MS);
    preview.show(level(0.3), []);
    expect(levels()).toEqual([0.1, 0.3]);
  });

  it('renews what it shows until stopped, then gives the lights back once', () => {
    const { preview, levels } = setup();
    preview.show(level(0.5), []);
    vi.advanceTimersByTime(PREVIEW_EVERY_MS * 2);
    expect(levels()).toEqual([0.5, 0.5, 0.5]);
    preview.stop();
    preview.stop();
    vi.advanceTimersByTime(PREVIEW_EVERY_MS * 3);
    expect(levels()).toEqual([0.5, 0.5, 0.5, 'clear_preview']);
    // Shown again, the same values are sent again.
    preview.show(level(0.5), []);
    expect(levels().at(-1)).toBe(0.5);
    preview.stop();
  });

  it('has nothing to give back when it never showed anything', () => {
    const { preview, levels } = setup();
    preview.stop();
    expect(levels()).toEqual([]);
  });
});
