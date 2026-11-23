import { describe, expect, it } from 'vitest';
import { channelMap, colorSequence, frameFromFunctions, functionsUnder, functionColor, groupFixtureIds, groupRows, groupWithFixtures, conflictsWith, maxSequenceColors, nextFreeAddress, parseTime, hexToColor, colorToHex } from './show';
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

describe('functionColor', () => {
  const scene = (values: unknown[]) => ({ id: 1, name: 'S', folder: null, kind: { type: 'scene', values, timing: {}, spread: { type: 'none' }, attribute_timing: [] } }) as never;
  const target = { type: 'group', id: 1 } as const;
  const red = { r: 1, g: 0, b: 0 };
  it('finds the color of a scene, null otherwise', () => {
    const s = {} as Show;
    const withColor = scene([
      { target, value: { type: 'explicit', value: { type: 'intensity', level: 1 } } },
      { target, value: { type: 'explicit', value: { type: 'color', color: red } } },
    ]);
    expect(functionColor(s, withColor)).toEqual(red);
    expect(functionColor(s, scene([{ target, value: { type: 'explicit', value: { type: 'intensity', level: 1 } } }]))).toBeNull();
    expect(functionColor(s, undefined)).toBeNull();
  });
});

describe('frameFromFunctions', () => {
  const sc = (id: number, name: string, color?: { r: number; g: number; b: number }) => ({
    id,
    name,
    folder: null,
    kind: {
      type: 'scene',
      values: color ? [{ target: { type: 'group', id: 1 }, value: { type: 'explicit', value: { type: 'color', color } } }] : [],
      timing: {},
      spread: { type: 'none' },
      attribute_timing: [],
    },
  });
  const s = { functions: [sc(1, 'Red', { r: 1, g: 0, b: 0 }), sc(2, 'Plain'), sc(3, 'Blue', { r: 0, g: 0, b: 1 })], palettes: [] } as unknown as Show;

  it('makes one bound button per function, in the given order', () => {
    const w = frameFromFunctions(s, [3, 1, 2], { solo: false, name: 'F', firstId: 10, x: 5, y: 6 });
    expect(w.kind).toEqual({ type: 'frame', pages: 1 });
    expect(w.id).toBe(10);
    expect(w.children.map((c) => [c.id, c.function, c.props.label])).toEqual([[11, 3, 'Blue'], [12, 1, 'Red'], [13, 2, 'Plain']]);
    expect(w.children[0]!.props.color).toEqual({ r: 0, g: 0, b: 255 });
    expect(w.children[2]!.props.color).toBeNull();
    expect(w.children.every((c) => c.kind.type === 'button')).toBe(true);
  });

  it('solo frame, grid size and unknown ids', () => {
    const w = frameFromFunctions(s, [1, 99, 2, 3], { solo: true, name: 'S', firstId: 1, x: 0, y: 0 });
    expect(w.kind).toEqual({ type: 'solo_frame' });
    expect(w.children).toHaveLength(3);
    expect(w.props.width).toBe(10 + 3 * 110);
    expect(w.props.height).toBe(24 + 80);
  });
});

describe('function tree helpers', () => {
  const fn = (id: number, name: string, folder: number | null, type = 'scene') => ({ id, name, folder, kind: { type } }) as never;
  const s = {
    functions: [fn(1, 'b', null), fn(2, 'a', 10), fn(3, 'c', 11), fn(4, 'x', null, 'chaser')],
    function_folders: [
      { id: 10, category: 'scene', name: 'F', parent: null },
      { id: 11, category: 'scene', name: 'G', parent: 10 },
    ],
  } as unknown as Show;
  it('lists a type, folder by folder', () => {
    expect(functionsUnder(s, 'scene', null).map((f) => f.id)).toEqual([1, 2, 3]);
    expect(functionsUnder(s, 'scene', 10).map((f) => f.id)).toEqual([2, 3]);
    expect(functionsUnder(s, 'chaser', null).map((f) => f.id)).toEqual([4]);
  });
});

describe('group helpers', () => {
  const g = { id: 1, name: 'G', members: [{ fixture: 1, head: 0, grid: { x: 0, y: 0 } }], grid_width: 1, grid_height: 1 };
  it('adds the heads of fixtures once and lists fixtures once', () => {
    const s = show([1, 10, 20]);
    const next = groupWithFixtures(s, g, [1, 2, 3]);
    expect(groupFixtureIds(next)).toEqual([1, 2, 3]);
    expect(next.members).toHaveLength(3);
    expect(groupWithFixtures(s, next, [2, 3])).toBe(next); // nothing new
  });
});

describe('groupRows', () => {
  it('lists the fixtures of a group in a universe, in grid order', () => {
    const s = show([1, 10, 20]);
    const g = {
      id: 1,
      name: 'G',
      members: [
        { fixture: 3, head: 0, grid: { x: 0, y: 0 } },
        { fixture: 1, head: 0, grid: { x: 1, y: 0 } },
      ],
      grid_width: 2,
      grid_height: 1,
    };
    expect(groupRows(s, g, 1).map((r) => [r.fixture.id, r.head])).toEqual([[3, null], [1, null]]);
    expect(groupRows(s, g, 2)).toEqual([]);
  });
});
