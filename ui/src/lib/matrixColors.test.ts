import { describe, expect, it } from 'vitest';
import { carryColors } from './matrixColors';
import type { Color } from './protocol';

const red: Color = { r: 1, g: 0, b: 0 };
const blue: Color = { r: 0, g: 0, b: 1 };
const black: Color = { r: 0, g: 0, b: 0 };
const white: Color = { r: 1, g: 1, b: 1 };
const green: Color = { r: 0, g: 1, b: 0 };

describe('carryColors', () => {
  it('keeps both colors from a chase to a gradient', () => {
    const next = carryColors({ type: 'chase', color: green, background: blue, width: 1 }, { type: 'gradient', from: red, to: black });
    expect(next).toEqual({ type: 'gradient', from: green, to: blue });
  });

  it('keeps both colors from a gradient to a fill', () => {
    const next = carryColors({ type: 'gradient', from: green, to: blue }, { type: 'fill', color: red, background: black });
    expect(next).toEqual({ type: 'fill', color: green, background: blue });
  });

  it('gives the color of a solid to the main color and leaves the second one as it is', () => {
    const next = carryColors({ type: 'solid', color: green }, { type: 'chase', color: red, background: black, width: 1 });
    expect(next).toEqual({ type: 'chase', color: green, background: black, width: 1 });
  });

  it('drops the second color when the new pattern has only one', () => {
    const next = carryColors({ type: 'fill', color: green, background: blue }, { type: 'text', text: 'KLIGHT', color: red });
    expect(next).toEqual({ type: 'text', text: 'KLIGHT', color: green });
  });

  it('keeps what is not a color of the new pattern, and does not share the colors', () => {
    const old = { type: 'solid', color: green } as const;
    const next = carryColors(old, { type: 'twinkle', color: white, background: black, density: 0.3 });
    expect(next).toEqual({ type: 'twinkle', color: green, background: black, density: 0.3 });
    if (next.type === 'twinkle') expect(next.color).not.toBe(old.color);
  });

  it('wave has one color', () => {
    expect(carryColors({ type: 'chase', color: green, background: blue, width: 1 }, { type: 'wave', color: red, wavelength: 4 })).toEqual({
      type: 'wave',
      color: green,
      wavelength: 4,
    });
  });
});
