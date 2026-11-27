import { describe, expect, it } from 'vitest';
import { lightSections } from './lightSections';
import type { PatchedFixture } from './protocol';

const fixture = (id: number, name: string, definition: string): PatchedFixture => ({
  id,
  name,
  definition,
  mode: 'm',
  universe: 1,
  address: id,
  orientation: { invert_pan: false, invert_tilt: false, swap_pan_tilt: false, upside_down: false },
  calibration: null,
  position_limits: null,
});

const all = [
  fixture(1, 'Spot 10', 'Acme/Spot'),
  fixture(2, 'Left', 'Acme/Par'),
  fixture(3, 'Spot 2', 'Acme/Spot'),
  fixture(4, 'Spot 9', 'Acme/Spot'),
  fixture(5, 'Right', 'Other/Par'),
];

const names = (query?: string) => lightSections(all, query).map((s) => [s.title, s.fixtures.map((f) => f.name)]);

describe('light sections', () => {
  it('has one per model, sorted, with numbers in order', () => {
    expect(names()).toEqual([
      ['Acme Par', ['Left']],
      ['Other Par', ['Right']],
      ['Spot', ['Spot 2', 'Spot 9', 'Spot 10']],
    ]);
  });

  it('keeps what matches a name or a model, ignoring case and spaces around', () => {
    expect(names(' spot 1')).toEqual([['Spot', ['Spot 10']]]);
    expect(names('OTHER')).toEqual([['Other Par', ['Right']]]);
    expect(names('nothing')).toEqual([]);
  });
});
