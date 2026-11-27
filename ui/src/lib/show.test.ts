import { describe, expect, it } from 'vitest';
import { beatFlash } from './beat';
import { channelMap, colorSequence, frameFromFunctions, frameKind, functionsUnder, functionColor, groupFixtureIds, groupRows, groupWithFixtures, groupsWithoutFixtures, conflictsWith, mappingConflicts, maxSequenceColors, nextFreeAddress, parseTime, parseSeconds, formatSeconds, formatBeats, formatTime, beatChoices, timeChoiceIndex, hexToColor, colorToHex } from './show';
import { sceneValueColor } from './core';
import type { InputMapping, Show, Widget } from './protocol';

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
      position_limits: null,
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

  it('cue times: a bare number is in seconds, and they are written back so', () => {
    expect(parseSeconds('2')).toEqual({ ms: 2000 });
    expect(parseSeconds('0.25')).toEqual({ ms: 250 });
    expect(parseSeconds('300ms')).toEqual({ ms: 300 });
    expect(parseSeconds('1.5 s')).toEqual({ ms: 1500 });
    expect(parseSeconds('4 beats')).toEqual({ beats: 4 });
    expect(parseSeconds('soon')).toBeNull();
    expect(formatSeconds({ ms: 2000 })).toBe('2 s');
    expect(formatSeconds({ ms: 250 })).toBe('0.25 s');
    expect(formatSeconds({ ms: 0 })).toBe('0 s');
    expect(formatSeconds({ beats: 0.5 })).toBe('1/2 beat');
    for (const t of [{ ms: 1234 }, { ms: 0 }, { beats: 2 }]) expect(parseSeconds(formatSeconds(t))).toEqual(t);
  });

  it('round-trips colors through linear RGB', () => {
    for (const hex of ['#ff0000', '#123456', '#808080', '#ffffff', '#000000']) {
      expect(colorToHex(hexToColor(hex))).toBe(hex);
    }
  });
});

import { moveInGrid, resizeGrid } from './show';
import { copyName } from './show';

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

const ZERO = { ms: 0 };
/** A scene function for a full show (the core reads the whole show). */
const sceneFn = (id: number, name: string, values: unknown[]) =>
  ({ id, name, folder: null, kind: { type: 'scene', values, timing: { delay: ZERO, fade_in: ZERO, fade_out: ZERO }, spread: { type: 'none' }, attribute_timing: [] } }) as never;

describe('functionColor (from the core)', () => {
  const target = { type: 'fixture', id: 1 } as const;
  const red = { r: 1, g: 0, b: 0 };
  const blue = { r: 0, g: 0, b: 1 };
  it('finds the color of a scene, palettes included, null otherwise', () => {
    const s: Show = {
      ...show([1, 10]),
      palettes: [{ id: 5, name: 'P', kind: 'Color', generic: [{ type: 'color', color: red }], per_fixture: [{ fixture: 2, values: [{ type: 'color', color: blue }] }] }],
      functions: [
        sceneFn(1, 'A', [
          { target, value: { type: 'explicit', value: { type: 'intensity', level: 1 } } },
          { target, value: { type: 'explicit', value: { type: 'color', color: red } } },
        ]),
        sceneFn(2, 'B', [{ target, value: { type: 'explicit', value: { type: 'intensity', level: 1 } } }]),
        // A palette with a value of its own for fixture 2 (the old TypeScript
        // version only looked at the generic value).
        sceneFn(3, 'C', [{ target: { type: 'fixture', id: 2 }, value: { type: 'palette', id: 5 } }]),
      ],
    };
    expect(functionColor(s, s.functions[0])).toEqual(red);
    expect(functionColor(s, s.functions[1])).toBeNull();
    expect(functionColor(s, s.functions[2])).toEqual(blue);
    expect(functionColor(s, undefined)).toBeNull();
    expect(sceneValueColor(s, 1, 0)).toBeNull();
    expect(sceneValueColor(s, 1, 1)).toEqual(red);
    expect(sceneValueColor(s, 3, 0)).toEqual(blue);
  });
});

describe('frameFromFunctions', () => {
  const sc = (id: number, name: string, color?: { r: number; g: number; b: number }) =>
    sceneFn(id, name, color ? [{ target: { type: 'fixture', id: 1 }, value: { type: 'explicit', value: { type: 'color', color } } }] : []);
  const s: Show = { ...show([1]), functions: [sc(1, 'Red', { r: 1, g: 0, b: 0 }), sc(2, 'Plain'), sc(3, 'Blue', { r: 0, g: 0, b: 1 })] };

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

describe('groupsWithoutFixtures', () => {
  // Regression: a group whose last member was removed vanished from the
  // fixture list and could no longer be selected or deleted.
  it('finds empty groups and groups whose fixtures are gone', () => {
    const s = show([1]);
    s.groups = [
      { id: 1, name: 'Full', members: [{ fixture: 1, head: 0, grid: null }], grid_width: 1, grid_height: 1 },
      { id: 2, name: 'Empty', members: [], grid_width: 1, grid_height: 1 },
      { id: 3, name: 'Gone', members: [{ fixture: 9, head: 0, grid: null }], grid_width: 1, grid_height: 1 },
    ];
    expect(groupsWithoutFixtures(s).map((g) => g.id)).toEqual([2, 3]);
  });
});

describe('mappingConflicts', () => {
  const note = { type: 'midi_note', channel: 0, note: 36 } as const;
  const mappings: InputMapping[] = [
    { id: 1, device: 'pad', message: note, target: { type: 'widget', id: 4 } },
    { id: 2, device: 'pad', message: { ...note, note: 37 }, target: { type: 'widget', id: 5 } },
    { id: 3, device: 'other', message: note, target: { type: 'blackout' } },
  ];
  it('tells a repeat from a control already used elsewhere', () => {
    expect(mappingConflicts(mappings, 'pad', note, { type: 'widget', id: 4 })).toEqual({ same: true, others: [] });
    expect(mappingConflicts(mappings, 'pad', note, { type: 'widget', id: 5 }).others.map((x) => x.id)).toEqual([1]);
    expect(mappingConflicts(mappings, 'pad', { ...note, note: 40 }, { type: 'tap_tempo' })).toEqual({ same: false, others: [] });
  });
});

describe('frameKind', () => {
  const child = (page: number): Widget => ({ id: page + 10, kind: { type: 'label' }, props: { x: 0, y: 0, width: 1, height: 1, label: '', color: null, page }, function: null, children: [] });
  const frame = (kind: Widget['kind'], pages: number[]): Widget => ({ ...child(0), id: 1, kind, children: pages.map(child) });
  // Regression: switching to solo frame and back reset a frame to one page.
  it('keeps the pages a frame needs', () => {
    expect(frameKind(frame({ type: 'frame', pages: 3 }, [0, 2]), 'solo_frame')).toEqual({ type: 'solo_frame' });
    expect(frameKind(frame({ type: 'solo_frame' }, [0, 2]), 'frame')).toEqual({ type: 'frame', pages: 3 });
    expect(frameKind(frame({ type: 'frame', pages: 4 }, [0]), 'frame')).toEqual({ type: 'frame', pages: 4 });
    expect(frameKind(frame({ type: 'solo_frame' }, []), 'frame')).toEqual({ type: 'frame', pages: 1 });
  });
});

describe('beat flash', () => {
  const a = { beat: 8, at: 1000, bpm: 120, beatsPerBar: 4 }; // a beat every 500 ms
  it('lights at the start of each beat and marks the first beat of a bar', () => {
    expect(beatFlash(a, 1000)).toEqual({ on: true, down: true }); // beat 8, bar start
    expect(beatFlash(a, 1100)).toEqual({ on: true, down: true }); // 100 ms in
    expect(beatFlash(a, 1300).on).toBe(false);
    expect(beatFlash(a, 1500)).toEqual({ on: true, down: false }); // beat 9
    expect(beatFlash(a, 3000)).toEqual({ on: true, down: true }); // beat 12
  });
  it('is off without a tempo', () => {
    expect(beatFlash(null, 0).on).toBe(false);
    expect(beatFlash({ ...a, bpm: 0 }, 1000).on).toBe(false);
  });
});

describe('times', () => {
  it('writes short beat lengths as fractions', () => {
    expect([0.03125, 0.5, 1, 4, 1.5, 0.3].map(formatBeats)).toEqual(['1/32 beat', '1/2 beat', '1 beat', '4 beats', '1.5 beats', '0.3 beats']);
    expect(formatTime({ beats: 0.125 })).toBe('1/8 beat');
    expect(formatTime({ ms: 500 })).toBe('500 ms');
  });
  it('offers beat lengths from 1/32 to 32, doubling', () => {
    expect(beatChoices()).toEqual([1 / 32, 1 / 16, 1 / 8, 1 / 4, 1 / 2, 1, 2, 4, 8, 16, 32]);
  });
  it('finds a time among the choices, or not', () => {
    const steps = beatChoices();
    const fades = [0, ...steps];
    expect(timeChoiceIndex(steps, { beats: 1 / 32 })).toBe(0);
    expect(timeChoiceIndex(steps, { beats: 32 })).toBe(10);
    expect(timeChoiceIndex(fades, { beats: 1 / 4 })).toBe(4);
    // No time at all is the fade's "0", in either unit.
    expect(timeChoiceIndex(fades, { ms: 0 })).toBe(0);
    expect(timeChoiceIndex(fades, { beats: 0 })).toBe(0);
    expect(timeChoiceIndex(steps, { ms: 0 })).toBe(-1);
    // Older shows: milliseconds and other lengths are not in the list.
    expect(timeChoiceIndex(fades, { ms: 500 })).toBe(-1);
    expect(timeChoiceIndex(steps, { beats: 3 })).toBe(-1);
  });
});

describe('names of copies', () => {
  it('takes the first one free, without piling up "copy"', () => {
    expect(copyName('Intro', ['Intro'])).toBe('Intro copy');
    expect(copyName('Intro', ['Intro', 'Intro copy'])).toBe('Intro copy 2');
    expect(copyName('Intro copy', ['Intro', 'Intro copy'])).toBe('Intro copy 2');
    expect(copyName('Intro copy 2', ['Intro copy', 'Intro copy 2'])).toBe('Intro copy 3');
    // A gap is filled; "copy" inside a name is part of it.
    expect(copyName('Intro copy 3', ['Intro copy 3'])).toBe('Intro copy');
    expect(copyName('Copy room', ['Copy room'])).toBe('Copy room copy');
  });
});
