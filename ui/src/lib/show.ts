// Pure helpers over the show model (unit tested).

import type {
  FixtureDefinition,
  FixtureGroup,
  ModeDef,
  PatchedFixture,
  Show,
  TimeValue,
  AttributeValue,
  Target,
  Attribute,
} from './protocol';

export const UNIVERSE_SIZE = 512;

// Logic that must match the engine comes from the core (WebAssembly).
export {
  channelMap,
  colorSequence,
  colorToHex,
  conflictsWith,
  fixtureInfo,
  footprint,
  hasColorMixing,
  hasPosition,
  headCount,
  hexToColor,
  maxSequenceColors,
  nextFreeAddress,
} from './core';
import { colorToHex } from './core';

export function defKey(d: FixtureDefinition): string {
  return `${d.manufacturer}/${d.model}`;
}

export function definitionOf(show: Show, f: PatchedFixture): FixtureDefinition | undefined {
  return show.definitions.find((d) => defKey(d) === f.definition);
}

export function modeOf(show: Show, f: PatchedFixture): ModeDef | undefined {
  return definitionOf(show, f)?.modes.find((m) => m.name === f.mode);
}

export function nextId(items: Array<{ id: number }>): number {
  return items.reduce((m, x) => Math.max(m, x.id), 0) + 1;
}

export function formatTime(t: TimeValue): string {
  return 'ms' in t ? `${t.ms} ms` : `${t.beats} beats`;
}

export function parseTime(text: string): TimeValue | null {
  const s = text.trim().toLowerCase();
  const beats = /^(\d+(?:\.\d+)?)\s*(b|beat|beats)$/.exec(s);
  if (beats) return { beats: Number(beats[1]) };
  const sec = /^(\d+(?:\.\d+)?)\s*s$/.exec(s);
  if (sec) return { ms: Math.round(Number(sec[1]) * 1000) };
  const ms = /^(\d+)\s*(ms)?$/.exec(s);
  if (ms) return { ms: Number(ms[1]) };
  return null;
}

export function describeValue(v: AttributeValue): string {
  switch (v.type) {
    case 'intensity':
      return `Intensity ${Math.round(v.level * 100)}%`;
    case 'color':
      return `Color ${colorToHex(v.color)}`;
    case 'position':
      return `Pan ${v.pan.toFixed(1)}° Tilt ${v.tilt.toFixed(1)}°`;
    case 'position_fan':
      return `Fan ${v.pan_spread}°${v.tilt_spread ? ` / ${v.tilt_spread}°` : ''} ${v.mode === 'from_center' ? 'from center' : 'linear'} around ${v.pan.toFixed(1)}°/${v.tilt.toFixed(1)}°`;
    case 'gobo':
      return `Gobo ${v.name}`;
    case 'prism':
      return `Prism ${v.name}`;
    case 'zoom':
      return `Zoom ${v.degrees}°`;
    case 'strobe':
      return v.strobe.type === 'Hz' ? `Strobe ${v.strobe.hz} Hz` : `Shutter ${v.strobe.type}`;
    case 'raw':
      return `${v.attribute} = ${v.value}`;
  }
}

export function describeTarget(show: Show, t: Target): string {
  switch (t.type) {
    case 'fixture':
      return show.fixtures.find((f) => f.id === t.id)?.name ?? `Fixture ${t.id}`;
    case 'head':
      return `${show.fixtures.find((f) => f.id === t.id)?.name ?? t.id} head ${t.head + 1}`;
    case 'group':
      return show.groups.find((g) => g.id === t.id)?.name ?? `Group ${t.id}`;
  }
}

/** Capability names of a given attribute across fixtures (for gobo/prism pickers). */
export function capabilityNames(show: Show, fixtures: PatchedFixture[], attribute: Attribute): string[] {
  const names = new Set<string>();
  for (const f of fixtures) {
    const d = definitionOf(show, f);
    const m = modeOf(show, f);
    if (!d || !m) continue;
    for (const ci of m.channels) {
      const ch = d.channels[ci];
      if (ch && ch.attribute === attribute) for (const c of ch.capabilities) if (c.name) names.add(c.name);
    }
  }
  return [...names];
}

export function groupFixtures(show: Show, g: FixtureGroup): PatchedFixture[] {
  const ids = new Set(g.members.map((m) => m.fixture));
  return show.fixtures.filter((f) => ids.has(f.id));
}

/** Distinct colors used to show group membership. */
export const GROUP_COLORS = ['#4f9dff', '#f0b429', '#3ccf7a', '#ff6b9a', '#a77bff', '#2ad4d4', '#ff8a3d', '#c7d64f'];

export function groupColor(show: Show, groupId: number): string {
  const i = show.groups.findIndex((g) => g.id === groupId);
  return GROUP_COLORS[(i < 0 ? 0 : i) % GROUP_COLORS.length]!;
}

/** Groups a fixture belongs to. */
export function groupsOf(show: Show, fixtureId: number): FixtureGroup[] {
  return show.groups.filter((g) => g.members.some((m) => m.fixture === fixtureId));
}

/**
 * Moves a group member to a grid cell (swapping with the member already
 * there) and re-orders members row by row: the grid defines the order.
 */
export function moveInGrid(g: FixtureGroup, from: number, to: { x: number; y: number }): FixtureGroup {
  const members = g.members.map((m) => ({ ...m, grid: m.grid ? { ...m.grid } : null }));
  const moving = members[from];
  if (!moving) return g;
  const old = moving.grid ?? { x: 0, y: 0 };
  const other = members.find((m, i) => i !== from && m.grid && m.grid.x === to.x && m.grid.y === to.y);
  if (other) other.grid = { ...old };
  moving.grid = { ...to };
  return { ...g, members: sortByGrid(members) };
}

export function sortByGrid<T extends { grid: { x: number; y: number } | null }>(members: T[]): T[] {
  return [...members].sort((a, b) => (a.grid?.y ?? 1e9) - (b.grid?.y ?? 1e9) || (a.grid?.x ?? 1e9) - (b.grid?.x ?? 1e9));
}

/** Resizes the grid, moving members that fall outside into free cells. */
export function resizeGrid(g: FixtureGroup, width: number, height: number): FixtureGroup {
  width = Math.max(1, width);
  height = Math.max(1, height);
  while (width * height < g.members.length) height += 1;
  const used = new Set<string>();
  const members = g.members.map((m) => ({ ...m, grid: m.grid ? { ...m.grid } : null }));
  for (const m of members) {
    if (m.grid && m.grid.x < width && m.grid.y < height && !used.has(`${m.grid.x},${m.grid.y}`)) used.add(`${m.grid.x},${m.grid.y}`);
    else m.grid = null;
  }
  for (const m of members) {
    if (m.grid) continue;
    for (let i = 0; i < width * height; i++) {
      const p = { x: i % width, y: Math.floor(i / width) };
      if (!used.has(`${p.x},${p.y}`)) {
        used.add(`${p.x},${p.y}`);
        m.grid = p;
        break;
      }
    }
  }
  return { ...g, grid_width: width, grid_height: height, members: sortByGrid(members) };
}
