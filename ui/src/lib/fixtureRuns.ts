// Channels shown side by side, cut where the fixture changes, so that each
// fixture gets one heading over all its channels.
import type { ChannelInfo } from './protocol';

export interface FixtureRun {
  /** Stable while the shown channels change. */
  key: string;
  /** Empty for an unpatched address. */
  name: string;
  patched: boolean;
  /** Alternate tint, to tell neighbouring fixtures apart. */
  alt: boolean;
  addresses: number[];
}

/**
 * `addresses` (1-based, ascending) grouped by the fixture patched there, as
 * told by the core's channel map. Unpatched addresses stay on their own, so
 * that they flow one by one.
 */
export function fixtureRuns(addresses: number[], map: (ChannelInfo | null)[]): FixtureRun[] {
  const runs: FixtureRun[] = [];
  for (const address of addresses) {
    const info = map[address - 1] ?? null;
    const key = info ? `fixture ${info.fixture_index}` : `address ${address}`;
    const last = runs[runs.length - 1];
    if (last?.key === key) {
      last.addresses.push(address);
      continue;
    }
    runs.push({
      key,
      name: info?.fixture_name ?? '',
      patched: info !== null,
      alt: info !== null && info.fixture_index % 2 === 1,
      addresses: [address],
    });
  }
  return runs;
}
