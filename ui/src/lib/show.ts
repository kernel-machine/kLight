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
  Color,
  Function as Function_,
  InputMessage,
  FunctionCategory,
  FunctionFolder,
  Widget,
  WidgetKind,
  InputMapping,
  InputTarget,
  SceneValue,
} from './protocol';

export const UNIVERSE_SIZE = 512;

// Logic that must match the engine comes from the core (WebAssembly).
export {
  beatChoices,
  channelMap,
  chaserStepTypes,
  collectionMemberTypes,
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
import { colorToHex, functionColor as coreFunctionColor, headCount } from './core';

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

/** Name for a copy: `X copy`, then `X copy 2`…, the first one not taken. */
export function copyName(name: string, taken: string[]): string {
  const base = `${name.replace(/ copy( \d+)?$/, '')} copy`;
  const used = new Set(taken);
  let free = base;
  for (let n = 2; used.has(free); n++) free = `${base} ${n}`;
  return free;
}

export function formatTime(t: TimeValue): string {
  return 'ms' in t ? `${t.ms} ms` : formatBeats(t.beats);
}

/** `1/8 beat`, `1 beat`, `4 beats`: fractions of one over a whole number are written as such. */
export function formatBeats(beats: number): string {
  const parts = 1 / beats;
  if (beats > 0 && beats < 1 && Number.isInteger(parts)) return `1/${parts} beat`;
  return `${beats} ${beats === 1 ? 'beat' : 'beats'}`;
}

/**
 * Index of `t` among `choices` (beat lengths; 0 stands for no time at all),
 * or -1 when it is not one of them, as for milliseconds in an older show.
 */
export function timeChoiceIndex(choices: number[], t: TimeValue): number {
  if ('beats' in t) return choices.indexOf(t.beats);
  return t.ms === 0 ? choices.indexOf(0) : -1;
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

/** A cue time as written in the cue list: seconds, or beats. */
export function formatSeconds(t: TimeValue): string {
  if (!('ms' in t)) return formatBeats(t.beats);
  return `${Number((t.ms / 1000).toFixed(3))} s`;
}

/** A cue time typed in the cue list: a bare number is in seconds; "ms", "s" and "beats" as in {@link parseTime}. */
export function parseSeconds(text: string): TimeValue | null {
  const bare = /^(\d+(?:\.\d+)?)$/.exec(text.trim());
  if (bare) return { ms: Math.round(Number(bare[1]) * 1000) };
  return parseTime(text);
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
      if (v.attribute === 'Pan' || v.attribute === 'Tilt') return `${v.attribute} DMX ${v.value}`;
      return v.attribute === 'Shutter' ? `Strobe DMX ${v.value}` : `${v.attribute} = ${v.value}`;
  }
}

/** Color a function shows (first color its scene sets), from the core; null if none. */
/** A scene or cue value in words: the intention, the palette or the DMX value. */
export function describeSceneValue(show: Show, sv: SceneValue): string {
  const v = sv.value;
  switch (v.type) {
    case 'explicit':
      return describeValue(v.value);
    case 'palette':
      return `Palette "${show.palettes.find((p) => p.id === v.id)?.name ?? '?'}"`;
    case 'channel':
      return `Channel ${v.value}`;
  }
}

export function functionColor(show: Show, f: Function_ | undefined): Color | null {
  return f ? coreFunctionColor(show, f.id) : null;
}

/** Function types in the order the FX screen lists them. */
export const FUNCTION_CATEGORIES: Array<{ id: FunctionCategory; label: string; one: string; short: string }> = [
  { id: 'scene', label: 'Scenes', one: 'scene', short: 'Scene' },
  { id: 'chaser', label: 'Chasers', one: 'chaser', short: 'Chaser' },
  { id: 'cue_list', label: 'Cue lists', one: 'cue list', short: 'Cue list' },
  { id: 'movement_effect', label: 'Movements', one: 'movement', short: 'Movement' },
  { id: 'matrix_effect', label: 'Matrix effects', one: 'matrix effect', short: 'Matrix' },
  { id: 'collection', label: 'Collections', one: 'collection', short: 'Collection' },
];

const byName = (a: { name: string }, b: { name: string }) =>
  a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });

/** Subfolders of a folder (or top-level folders of a type), sorted by name. */
export function foldersIn(show: Show, category: FunctionCategory, parent: number | null): FunctionFolder[] {
  return show.function_folders.filter((f) => f.category === category && (f.parent ?? null) === parent).sort(byName);
}

/** Functions directly in a folder (or at the top level of a type), sorted by name. */
export function functionsIn(show: Show, category: FunctionCategory, folder: number | null): Function_[] {
  return show.functions.filter((f) => f.kind.type === category && (f.folder ?? null) === folder).sort(byName);
}

/** Every function in a folder or a type, subfolders included, in tree order. */
export function functionsUnder(show: Show, category: FunctionCategory, folder: number | null): Function_[] {
  return [
    ...functionsIn(show, category, folder),
    ...foldersIn(show, category, folder).flatMap((f) => functionsUnder(show, category, f.id)),
  ];
}

/** Short label of a mapped input message, e.g. "CC 7 ch 1". */
export function describeInputMessage(m: InputMessage): string {
  if (m.type === 'midi_note') return `Note ${m.note} ch ${m.channel + 1}`;
  if (m.type === 'midi_cc') return `CC ${m.controller} ch ${m.channel + 1}`;
  return `OSC ${m.path}`;
}

/** Widgets of the virtual console, frames' children included. */
export function allWidgets(widgets: Widget[]): Widget[] {
  return widgets.flatMap((w) => [w, ...allWidgets(w.children)]);
}

/** What a mapping drives, e.g. 'Widget "Red"' or "Grand master". */
export function describeInputTarget(show: Show, t: InputTarget): string {
  if (t.type === 'widget') return `Widget "${allWidgets(show.virtual_console.widgets).find((w) => w.id === t.id)?.props.label ?? t.id}"`;
  const label = t.type.replace(/_/g, ' ');
  return label.charAt(0).toUpperCase() + label.slice(1);
}

const sameJson = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Where a learned control is already used: `same` if it already drives
 * `target`, `others` = mappings of the same control to something else (its
 * LED can show only one of them: the first).
 */
export function mappingConflicts(
  mappings: InputMapping[],
  device: string,
  message: InputMessage,
  target: InputTarget,
): { same: boolean; others: InputMapping[] } {
  const ofControl = mappings.filter((m) => m.device === device && sameJson(m.message, message));
  return { same: ofControl.some((m) => sameJson(m.target, target)), others: ofControl.filter((m) => !sameJson(m.target, target)) };
}

/**
 * Kind of a frame switched to `type`. A frame keeps enough pages for every
 * child, so switching to a solo frame and back hides nothing.
 */
export function frameKind(w: Widget, type: 'frame' | 'solo_frame'): WidgetKind {
  if (type === 'solo_frame') return { type: 'solo_frame' };
  const used = Math.max(0, ...w.children.map((c) => c.props.page + 1));
  const had = w.kind.type === 'frame' ? w.kind.pages : 1;
  return { type: 'frame', pages: Math.max(1, had, used) };
}

export const FRAME_BUTTON = { width: 100, height: 70, gap: 10, columns: 4, header: 24 } as const;

/**
 * A frame (or solo frame) holding one button per function, in the given
 * order, laid out in a grid. Each button is bound to its function and takes
 * its name and, for a scene that sets a color, that color. Widget ids are
 * `firstId` for the frame and the following ones for the buttons.
 */
export function frameFromFunctions(
  show: Show,
  functionIds: number[],
  opts: { solo: boolean; name: string; firstId: number; x: number; y: number },
): Widget {
  const { width, height, gap, columns, header } = FRAME_BUTTON;
  const functions = functionIds.flatMap((id) => show.functions.filter((f) => f.id === id));
  const cols = Math.max(1, Math.min(columns, functions.length));
  const rows = Math.max(1, Math.ceil(functions.length / cols));
  const children: Widget[] = functions.map((f, i) => {
    const color = functionColor(show, f);
    const hex = color ? colorToHex(color) : null;
    return {
      id: opts.firstId + 1 + i,
      kind: { type: 'button', action: 'Toggle' },
      props: {
        x: gap + (i % cols) * (width + gap),
        y: header + Math.floor(i / cols) * (height + gap),
        width,
        height,
        label: f.name,
        color: hex ? { r: parseInt(hex.slice(1, 3), 16), g: parseInt(hex.slice(3, 5), 16), b: parseInt(hex.slice(5, 7), 16) } : null,
        page: 0,
      },
      function: f.id,
      children: [],
    };
  });
  return {
    id: opts.firstId,
    kind: opts.solo ? { type: 'solo_frame' } : { type: 'frame', pages: 1 },
    props: {
      x: opts.x,
      y: opts.y,
      width: gap + cols * (width + gap),
      height: header + rows * (height + gap),
      label: opts.name,
      color: null,
      page: 0,
    },
    function: null,
    children,
  };
}

export function describeTarget(show: Show, t: Target): string {
  switch (t.type) {
    case 'fixture':
      return show.fixtures.find((f) => f.id === t.id)?.name ?? `Fixture ${t.id}`;
    case 'head':
      return `${show.fixtures.find((f) => f.id === t.id)?.name ?? t.id} head ${t.head + 1}`;
    case 'group':
      return show.groups.find((g) => g.id === t.id)?.name ?? `Group ${t.id}`;
    case 'scene':
      return 'All lights';
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

/** Drag payload of fixture rows: a JSON array of fixture ids. */
export const FIXTURE_DRAG = 'application/x-klight-fixtures';

/** Fixture ids of a group, each once, in grid order. */
export function groupFixtureIds(g: FixtureGroup): number[] {
  return [...new Set(sortByGrid(g.members).map((m) => m.fixture))];
}

/** Groups with no fixture in any universe (empty, or their fixtures were deleted). */
export function groupsWithoutFixtures(show: Show): FixtureGroup[] {
  return show.groups.filter((g) => show.universes.every((u) => groupRows(show, g, u.id).length === 0));
}

/** A line of a group in the fixture list: a whole fixture, or one head of it. */
export interface GroupRow {
  fixture: PatchedFixture;
  /** `null` = every head of the fixture is in the group. */
  head: number | null;
}

/**
 * What a group lists under a universe, in grid order: each fixture once when
 * all its heads belong to the group, otherwise one line per head that does.
 */
export function groupRows(show: Show, g: FixtureGroup, universe: number): GroupRow[] {
  const rows: GroupRow[] = [];
  const done = new Set<number>();
  for (const m of sortByGrid(g.members)) {
    const fixture = show.fixtures.find((f) => f.id === m.fixture);
    if (!fixture || fixture.universe !== universe || done.has(fixture.id)) continue;
    done.add(fixture.id);
    const heads = g.members.filter((x) => x.fixture === fixture.id).map((x) => x.head).sort((a, b) => a - b);
    if (heads.length >= headCount(show, fixture)) rows.push({ fixture, head: null });
    else rows.push(...heads.map((head) => ({ fixture, head })));
  }
  return rows;
}

/** The group with every head of the given fixtures added (already members are skipped). */
export function groupWithFixtures(show: Show, g: FixtureGroup, fixtureIds: number[]): FixtureGroup {
  const existing = new Set(g.members.map((m) => `${m.fixture}:${m.head}`));
  const extra = show.fixtures
    .filter((f) => fixtureIds.includes(f.id))
    .flatMap((f) => Array.from({ length: headCount(show, f) }, (_, head) => ({ fixture: f.id, head, grid: null })))
    .filter((m) => !existing.has(`${m.fixture}:${m.head}`));
  if (!extra.length) return g;
  return resizeGrid({ ...g, members: [...g.members, ...extra] }, g.grid_width, g.grid_height);
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
