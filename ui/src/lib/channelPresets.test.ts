import { describe, expect, it } from 'vitest';
import { channelPresets } from './core';
import { hasPresets, presetAt, presetSwatch } from './channelIcon';
import type { Capability, ChannelDef, Show } from './protocol';

const cap = (min: number, max: number, name: string, kind: Capability['kind'] = { type: 'Generic' }): Capability => ({ min, max, name, kind });
const ch = (name: string, attribute: ChannelDef['attribute'], capabilities: Capability[] = [], byte: ChannelDef['byte'] = 'Coarse'): ChannelDef => ({
  name,
  attribute,
  byte,
  default: 0,
  capabilities,
});
const red = { r: 255, g: 0, b: 0 };
const blue = { r: 0, g: 0, b: 255 };

const wheel = [
  cap(0, 9, 'White', { type: 'Color', color: { r: 255, g: 255, b: 255 }, color2: null }),
  cap(10, 19, 'Red', { type: 'Color', color: red, color2: null }),
  cap(20, 29, 'Red/Blue', { type: 'Color', color: red, color2: blue }),
];

const show: Show = {
  schema_version: 1,
  name: 't',
  universes: [{ id: 1, name: 'U', output: null }],
  definitions: [
    {
      manufacturer: 'G',
      model: 'P',
      kind: 'Other',
      channels: [ch('Pan', 'Pan'), ch('Color Wheel', 'ColorWheel', wheel), ch('Color Wheel Fine', 'ColorWheel', wheel, 'Fine'), ch('Dimmer', 'Dimmer', [cap(0, 255, 'Intensity')])],
      modes: [{ name: 'm', channels: [0, 1, 2, 3], heads: [], physical: null }],
      physical: { pan_max_deg: 540, tilt_max_deg: 0, zoom_min_deg: 0, zoom_max_deg: 0 },
    },
  ],
  fixtures: [
    {
      id: 1,
      name: 'f',
      definition: 'G/P',
      mode: 'm',
      universe: 1,
      address: 10,
      orientation: { invert_pan: false, invert_tilt: false, swap_pan_tilt: false, upside_down: false },
      calibration: null,
      position_limits: null,
    },
  ],
  groups: [],
  palettes: [],
  functions: [],
  function_folders: [],
  virtual_console: { widgets: [] },
  input_devices: [],
  input_mappings: [],
  settings: { tick_hz: 44, design_pin: null, output_refresh_ms: 1000 },
};

describe('channel presets (from the core)', () => {
  it('lists the named ranges of a channel with the value that selects each', () => {
    const p = channelPresets(show, 1, 11);
    expect(p.map((x) => [x.name, x.min, x.max, x.value])).toEqual([
      ['White', 0, 9, 4],
      ['Red', 10, 19, 14],
      ['Red/Blue', 20, 29, 24],
    ]);
    expect(hasPresets(p)).toBe(true);
  });

  it('has none for channels without a choice, fine bytes and unpatched addresses', () => {
    expect(hasPresets(channelPresets(show, 1, 10))).toBe(false); // Pan
    expect(channelPresets(show, 1, 12)).toEqual([]); // fine byte
    expect(hasPresets(channelPresets(show, 1, 13))).toBe(false); // one range only
    expect(channelPresets(show, 1, 200)).toEqual([]);
    expect(channelPresets(show, 2, 11)).toEqual([]);
  });

  it('finds the preset a value is in and the color to show', () => {
    const p = channelPresets(show, 1, 11);
    expect(presetAt(p, 14)).toBe(1);
    expect(presetAt(p, 0)).toBe(0);
    expect(presetAt(p, 29)).toBe(2);
    expect(presetAt(p, 200)).toBe(-1);
    expect(presetSwatch(p[1]!.kind)).toBe('rgb(255, 0, 0)');
    expect(presetSwatch(p[2]!.kind)).toBe('linear-gradient(90deg, rgb(255, 0, 0) 50%, rgb(0, 0, 255) 50%)');
    expect(presetSwatch({ type: 'Open' })).toBeNull();
  });
});

describe('what a scene can offer for some fixtures (from the core)', () => {
  it('lists the colors a wheel-only fixture can show', async () => {
    const { colorChoices } = await import('./core');
    const c = colorChoices(show, [1]);
    // The two-color slot is not a color the engine snaps to.
    expect(c.wheel.map((w) => w.name)).toEqual(['White', 'Red']);
    expect(c.free).toBe(false);
    expect(colorChoices(show, [])).toEqual({ wheel: [], free: true });
  });

  it('lists the kinds of values the fixtures can act on', async () => {
    const { valueSupport } = await import('./core');
    expect(valueSupport(show, [1])).toEqual({ intensity: true, color: true, position: true, position_degrees: true, position_dmx: false, gobo: false, prism: false, zoom: false, strobe: false, strobe_hz: false, strobe_dmx: false });
    expect(valueSupport(show, [])).toMatchObject({ intensity: false, color: false, position: false });
  });
});
