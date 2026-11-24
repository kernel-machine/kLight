// Which icon a channel gets, from what it controls (`Attribute`). Pure and
// exhaustive: a new attribute in the core stops the UI from compiling until it
// is given an icon here.

import type { Attribute, CapabilityKind, ChannelPreset } from './protocol';

export type ChannelIconKind =
  | { type: 'bulb' }
  | { type: 'pan' }
  | { type: 'tilt' }
  | { type: 'strobe' }
  | { type: 'gobo'; rotating: boolean }
  | { type: 'wheel' }
  | { type: 'speed' }
  /** A square in the color of the channel with its initial, like QLC+. */
  | { type: 'color'; letter: string; fill: string; ink: string }
  | { type: 'generic' };

const DARK = '#1b1f27';
const LIGHT = '#ffffff';
const color = (letter: string, fill: string, ink: string): ChannelIconKind => ({ type: 'color', letter, fill, ink });

export function channelIcon(a: Attribute): ChannelIconKind {
  switch (a) {
    case 'Dimmer':
      return { type: 'bulb' };
    case 'Pan':
      return { type: 'pan' };
    case 'Tilt':
      return { type: 'tilt' };
    case 'Shutter':
      return { type: 'strobe' };
    case 'Gobo':
      return { type: 'gobo', rotating: false };
    case 'GoboRotation':
      return { type: 'gobo', rotating: true };
    case 'ColorWheel':
      return { type: 'wheel' };
    case 'Speed':
      return { type: 'speed' };
    case 'Red':
      return color('R', '#e5392f', LIGHT);
    case 'Green':
      return color('G', '#27b34b', LIGHT);
    case 'Blue':
      return color('B', '#2f6bf0', LIGHT);
    case 'White':
      return color('W', '#f4f4f0', DARK);
    case 'Amber':
      return color('A', '#ffb000', DARK);
    case 'Uv':
      return color('UV', '#7b3fe4', LIGHT);
    case 'Lime':
      return color('L', '#a6e22e', DARK);
    case 'Indigo':
      return color('I', '#3f3fb8', LIGHT);
    case 'Cyan':
      return color('C', '#19c3d9', DARK);
    case 'Magenta':
      return color('M', '#e0258f', LIGHT);
    case 'Yellow':
      return color('Y', '#ffe600', DARK);
    case 'ColorTemperature':
    case 'Prism':
    case 'PrismRotation':
    case 'Zoom':
    case 'Focus':
    case 'Iris':
    case 'Frost':
    case 'Effect':
    case 'Maintenance':
    case 'NoFunction':
    case 'Other':
      return { type: 'generic' };
  }
}

/** Short name of what the icon stands for (tooltip). */
export function channelIconLabel(a: Attribute): string {
  switch (channelIcon(a).type) {
    case 'bulb':
      return 'Intensity';
    case 'pan':
      return 'Pan';
    case 'tilt':
      return 'Tilt';
    case 'strobe':
      return 'Shutter / strobe';
    case 'gobo':
      return a === 'GoboRotation' ? 'Gobo rotation' : 'Gobo';
    case 'wheel':
      return 'Color wheel';
    case 'speed':
      return 'Speed';
    case 'color':
      return a;
    case 'generic':
      return a.replace(/([a-z])([A-Z])/g, '$1 $2');
  }
}

const css = (c: { r: number; g: number; b: number }) => `rgb(${c.r}, ${c.g}, ${c.b})`;

/** CSS background showing a preset's color (split slots show both colors), or null. */
export function presetSwatch(kind: CapabilityKind): string | null {
  if (kind.type !== 'Color') return null;
  if (!kind.color2) return css(kind.color);
  return `linear-gradient(90deg, ${css(kind.color)} 50%, ${css(kind.color2)} 50%)`;
}

/** Index of the preset whose range holds `value`, or -1. */
export function presetAt(presets: ChannelPreset[], value: number): number {
  return presets.findIndex((p) => value >= p.min && value <= p.max);
}

/** A channel offers a choice only if it has more than one named range. */
export const hasPresets = (presets: ChannelPreset[]): boolean => presets.length > 1;
