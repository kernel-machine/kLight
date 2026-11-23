import { describe, expect, it } from 'vitest';
import { channelMap, colorSequence, conflictsWith, maxSequenceColors, nextFreeAddress, parseTime, hexToColor, colorToHex } from './show';
import type { Show } from './protocol';

const def = {
  manufacturer: 'G',
  model: 'P',
  kind: 'Other' as const,
  channels: [],
  modes: [{ name: 'm', channels: [0, 1, 2, 3, 4, 5], heads: [], physical: null }],
  physical: { pan_max_deg: 0, tilt_max_deg: 0, zoom_min_deg: 0, zoom_max_deg: 0 },
};

function show(addresses: number[]): Show {
  return {
    schema_version: 1,
    name: 't',
    universes: [{ id: 1, name: 'U', output: null }],
    definitions: [def],
    fixtures: addresses.map((a, i) => ({
      id: i + 1,
      name: `f${i}`,
      definition: 'G/P',
      mode: 'm',
      universe: 1,
      address: a,
      orientation: { invert_pan: false, invert_tilt: false, swap_pan_tilt: false, upside_down: false },
      calibration: null,
    })),
    groups: [],
    palettes: [],
    functions: [],
    function_folders: [],
    virtual_console: { widgets: [] },
    input_devices: [],
    input_mappings: [],
    settings: { tick_hz: 44, design_pin: null, output_refresh_ms: 1000 },
  };
}

describe('patch helpers (core WebAssembly)', () => {
  it('maps the channels of a universe', () => {
    const map = channelMap(show([5]), 1);
    expect(map).toHaveLength(512);
    expect(map[3]).toBeNull();
    expect(map[4]).toMatchObject({ fixture: 1, offset: 0, fixture_name: 'f0' });
    expect(map[9]).toMatchObject({ offset: 5 });
  });

  it('finds the next free address', () => {
    expect(nextFreeAddress(show([]), 1, 6)).toBe(1);
    expect(nextFreeAddress(show([1]), 1, 6)).toBe(7);
    expect(nextFreeAddress(show([1, 13]), 1, 6)).toBe(7);
    expect(nextFreeAddress(show([1, 10]), 1, 6)).toBe(16);
    expect(nextFreeAddress(show([]), 1, 6, 508)).toBeNull();
  });

  it('detects conflicts', () => {
    expect(conflictsWith(show([1, 20]), 1, 5, 6)).toEqual([1]);
    expect(conflictsWith(show([1, 20]), 1, 7, 6)).toEqual([]);
  });
});

describe('misc helpers', () => {
  it('parses times', () => {
    expect(parseTime('500')).toEqual({ ms: 500 });
    expect(parseTime('1.5s')).toEqual({ ms: 1500 });
    expect(parseTime('2 beats')).toEqual({ beats: 2 });
    expect(parseTime('abc')).toBeNull();
  });

  it('round-trips colors through linear RGB', () => {
    for (const hex of ['#ff0000', '#123456', '#808080', '#ffffff', '#000000']) {
      expect(colorToHex(hexToColor(hex))).toBe(hex);
    }
  });
});

import { moveInGrid, resizeGrid } from './show';

describe('group grid', () => {
  const g = {
    id: 1,
    name: 'g',
    grid_width: 3,
    grid_height: 1,
    members: [0, 1, 2].map((i) => ({ fixture: i + 1, head: 0, grid: { x: i, y: 0 } })),
  };

  it('dragging onto an occupied cell swaps and reorders', () => {
    const r = moveInGrid(g, 0, { x: 2, y: 0 });
    expect(r.members.map((m) => m.fixture)).toEqual([3, 2, 1]);
  });

  it('dragging onto an empty cell moves', () => {
    const r = moveInGrid({ ...g, grid_height: 2 }, 1, { x: 0, y: 1 });
    expect(r.members.map((m) => m.fixture)).toEqual([1, 3, 2]);
    expect(r.members[2]!.grid).toEqual({ x: 0, y: 1 });
  });

  it('shrinking the grid keeps every member inside', () => {
    const r = resizeGrid(g, 2, 1);
    expect(r.grid_height).toBe(2);
    expect(r.members.every((m) => m.grid!.x < 2 && m.grid!.y < 2)).toBe(true);
    expect(new Set(r.members.map((m) => `${m.grid!.x},${m.grid!.y}`)).size).toBe(3);
  });
});

describe('color sequence (from the core)', () => {
  it('gives named colors, capped at the maximum', () => {
    expect(colorSequence(3).map((c) => c.name)).toEqual(['Red', 'Green', 'Blue']);
    expect(colorSequence(1000)).toHaveLength(maxSequenceColors());
    expect(colorToHex(colorSequence(1)[0]!.color)).toBe('#ff0000');
  });
});
