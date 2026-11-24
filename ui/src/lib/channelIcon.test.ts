import { describe, expect, it } from 'vitest';
import { channelIcon, channelIconLabel } from './channelIcon';
import type { Attribute } from './protocol';

// Every attribute of the core (a missing one is a type error in channelIcon).
const ALL: Attribute[] = [
  'Dimmer', 'Red', 'Green', 'Blue', 'White', 'Amber', 'Uv', 'Lime', 'Indigo', 'Cyan', 'Magenta', 'Yellow',
  'ColorTemperature', 'Pan', 'Tilt', 'ColorWheel', 'Gobo', 'GoboRotation', 'Prism', 'PrismRotation', 'Shutter',
  'Zoom', 'Focus', 'Iris', 'Frost', 'Speed', 'Effect', 'Maintenance', 'NoFunction', 'Other',
];

describe('channel icons', () => {
  it('gives the requested icon to each kind of channel', () => {
    expect(channelIcon('Dimmer').type).toBe('bulb');
    expect(channelIcon('Pan').type).toBe('pan');
    expect(channelIcon('Tilt').type).toBe('tilt');
    expect(channelIcon('Shutter').type).toBe('strobe');
    expect(channelIcon('Gobo')).toEqual({ type: 'gobo', rotating: false });
    expect(channelIcon('GoboRotation')).toEqual({ type: 'gobo', rotating: true });
    expect(channelIcon('ColorWheel').type).toBe('wheel');
    expect(channelIcon('Speed').type).toBe('speed');
  });

  it('colors each emitter with its own square', () => {
    const squares = (['Red', 'Green', 'Blue', 'White', 'Amber', 'Uv', 'Lime', 'Indigo', 'Cyan', 'Magenta', 'Yellow'] as const).map((a) => channelIcon(a));
    for (const s of squares) expect(s.type).toBe('color');
    const fills = squares.map((s) => (s.type === 'color' ? s.fill : ''));
    expect(new Set(fills).size).toBe(fills.length);
    expect(channelIcon('Red')).toMatchObject({ letter: 'R', fill: '#e5392f' });
    expect(channelIcon('Amber')).toMatchObject({ letter: 'A' });
    expect(channelIcon('Uv')).toMatchObject({ letter: 'UV' });
  });

  it('uses a generic icon for the rest, and every attribute has a label', () => {
    for (const a of ['Zoom', 'Focus', 'Iris', 'Prism', 'Effect', 'Maintenance', 'Other'] as const) {
      expect(channelIcon(a).type).toBe('generic');
    }
    for (const a of ALL) expect(channelIconLabel(a)).not.toBe('');
    expect(channelIconLabel('PrismRotation')).toBe('Prism Rotation');
  });
});
