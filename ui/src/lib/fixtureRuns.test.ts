import { describe, expect, it } from 'vitest';
import { fixtureRuns } from './fixtureRuns';
import type { ChannelInfo } from './protocol';

const channel = (fixture_index: number, fixture_name: string, offset: number): ChannelInfo => ({
  fixture: fixture_index + 1,
  fixture_name,
  fixture_index,
  offset,
  channel_name: `Channel ${offset + 1}`,
  attribute: 'Dimmer',
});

describe('fixture runs', () => {
  // Two fixtures back to back, a gap of two addresses, a third one.
  const map = [channel(0, 'Spot', 0), channel(0, 'Spot', 1), channel(1, 'Spot', 0), null, null, channel(2, 'Wash', 0), channel(2, 'Wash', 1)];

  it('groups the channels of each fixture, even of two with the same name', () => {
    const runs = fixtureRuns([1, 2, 3, 4, 5, 6, 7], map);
    expect(runs.map((r) => [r.name, r.addresses])).toEqual([
      ['Spot', [1, 2]],
      ['Spot', [3]],
      ['', [4]],
      ['', [5]],
      ['Wash', [6, 7]],
    ]);
    expect(runs.map((r) => r.patched)).toEqual([true, true, false, false, true]);
    expect(runs.map((r) => r.alt)).toEqual([false, true, false, false, false]);
    expect(new Set(runs.map((r) => r.key)).size).toBe(runs.length);
  });

  it('names a fixture whose first channel is not shown, under the same key', () => {
    const all = fixtureRuns([6, 7], map);
    const some = fixtureRuns([2, 7], map);
    expect(some.map((r) => [r.name, r.addresses])).toEqual([
      ['Spot', [2]],
      ['Wash', [7]],
    ]);
    expect(some[1]?.key).toBe(all[0]?.key);
  });
});
