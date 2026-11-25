// Core logic in the browser: the engine's own Rust functions compiled to
// WebAssembly (crates/klight-wasm). The UI never re-implements them, so
// previews and patch checks match the engine by construction.
//
// Call `initCore()` once before using anything else.

import init, { initSync, ShowView, MovementView, MatrixView, linear_to_hex, hex_to_linear, set_scene_channel, max_sequence_colors, color_sequence_json, beat_flash_bits, widget_light_code, chaser_step_types, beat_choices } from './wasm/klight_wasm.js';
import type {
  ActiveFunction,
  FunctionCategory,
  WidgetKind,
  ChannelInfo,
  ChannelPreset,
  ChannelRef,
  ColorChoices,
  ValueSupport,
  Color,
  FixtureGroup,
  FixtureInfo,
  SceneFixtureChannels,
  SceneValue,
  GroupLayout,
  MatrixEffect,
  MovementEffect,
  PatchedFixture,
  Show,
} from './protocol';

export async function initCore(): Promise<void> {
  await init();
}

/** For tests (Node): initialise from the module bytes. */
export function initCoreSync(bytes: BufferSource): void {
  initSync({ module: bytes });
}

// One view of the current show; rebuilt only when the show object changes
// (the client keeps unchanged objects identical).
let cached: {
  show: Show;
  view: ShowView;
  channels: Map<number, Array<ChannelInfo | null>>;
  infos: Map<number, FixtureInfo>;
  sceneChannels: Map<number, SceneFixtureChannels[]>;
  presets: Map<string, ChannelPreset[]>;
} | null = null;

function view(show: Show) {
  if (cached?.show !== show) {
    cached?.view.free();
    cached = { show, view: new ShowView(JSON.stringify(show)), channels: new Map(), infos: new Map(), sceneChannels: new Map(), presets: new Map() };
  }
  return cached;
}

export function fixtureInfo(show: Show, id: number): FixtureInfo | null {
  const c = view(show);
  let info = c.infos.get(id);
  if (!info) {
    const json = c.view.fixture_info(id);
    if (json === undefined) return null;
    info = JSON.parse(json) as FixtureInfo;
    c.infos.set(id, info);
  }
  return info;
}

export const footprint = (show: Show, f: PatchedFixture) => fixtureInfo(show, f.id)?.footprint ?? 0;
export const headCount = (show: Show, f: PatchedFixture) => fixtureInfo(show, f.id)?.heads ?? 1;
export const hasPosition = (show: Show, f: PatchedFixture) => fixtureInfo(show, f.id)?.has_position ?? false;
export const hasColorMixing = (show: Show, f: PatchedFixture) => fixtureInfo(show, f.id)?.has_color_mixing ?? false;

/** First free address for `size` channels in a universe, or null. */
export function nextFreeAddress(show: Show, universe: number, size: number, from = 1): number | null {
  if (size <= 0 || size > 512) return null;
  return view(show).view.next_free_address(universe, size, Math.max(1, Math.min(512, from))) ?? null;
}

/** Fixtures that `size` channels at `address` would overlap. */
export function conflictsWith(show: Show, universe: number, address: number, size: number, exclude?: number): number[] {
  if (size <= 0 || address < 1 || address > 512) return [];
  return Array.from(view(show).view.overlapping(universe, address, Math.min(size, 512), exclude));
}

/** What is patched on each of the 512 addresses of a universe. */
export function channelMap(show: Show, universe: number): Array<ChannelInfo | null> {
  const c = view(show);
  let map = c.channels.get(universe);
  if (!map) {
    map = JSON.parse(c.view.channel_map(universe)) as Array<ChannelInfo | null>;
    c.channels.set(universe, map);
  }
  return map;
}

/**
 * Named ranges of the channel patched at `address` (color wheel slots, gobos,
 * shutter modes...) with the DMX value that selects each, as the engine
 * resolves them. Empty if the channel has none.
 */
export function channelPresets(show: Show, universe: number, address: number): ChannelPreset[] {
  const c = view(show);
  const key = `${universe}:${address}`;
  let p = c.presets.get(key);
  if (!p) {
    p = JSON.parse(c.view.channel_presets(universe, address)) as ChannelPreset[];
    c.presets.set(key, p);
  }
  return p;
}

/** Which kinds of scene values some fixtures can act on (the engine's own rules). */
export function valueSupport(show: Show, fixtureIds: number[]): ValueSupport {
  return JSON.parse(view(show).view.value_support(Uint32Array.from(fixtureIds))) as ValueSupport;
}

/**
 * Colors that make sense for some fixtures: the wheel colors of the ones
 * without color mixing, and whether any color is meaningful (`free`).
 */
export function colorChoices(show: Show, fixtureIds: number[]): ColorChoices {
  return JSON.parse(view(show).view.color_choices(Uint32Array.from(fixtureIds))) as ColorChoices;
}

/** All the channels of the fixtures a scene drives (engine's own compilation). */
export function sceneChannels(show: Show, functionId: number): SceneFixtureChannels[] {
  const c = view(show);
  let p = c.sceneChannels.get(functionId);
  if (!p) {
    const json = c.view.scene_channels(functionId);
    p = json ? (JSON.parse(json) as SceneFixtureChannels[]) : [];
    c.sceneChannels.set(functionId, p);
  }
  return p;
}

/**
 * Scene `values` after adding `incoming`: values in the same slot are replaced
 * and channels set by hand that `incoming` writes are released.
 */
export function addSceneValues(show: Show, values: SceneValue[], incoming: SceneValue[]): SceneValue[] {
  return JSON.parse(view(show).view.add_scene_values(JSON.stringify(values), JSON.stringify(incoming))) as SceneValue[];
}

/** Scene `values` with one channel of a fixture set by hand, or released (`null`). */
export function setSceneChannel(values: SceneValue[], fixture: number, channel: ChannelRef, value: number | null): SceneValue[] {
  return JSON.parse(set_scene_channel(JSON.stringify(values), fixture, JSON.stringify(channel), value)) as SceneValue[];
}

/** Color a function shows (first color its scene sets, palettes resolved as the engine does), or null. */
export function functionColor(show: Show, functionId: number): Color | null {
  return toColor(view(show).view.function_color(functionId));
}

/** Color the `index`-th value of a scene sets (palettes resolved as the engine does), or null. */
export function sceneValueColor(show: Show, functionId: number, index: number): Color | null {
  return toColor(view(show).view.scene_value_color(functionId, index));
}

function toColor(c: Float32Array | undefined): Color | null {
  return c && c.length >= 3 ? { r: c[0]!, g: c[1]!, b: c[2]! } : null;
}

/** Grid of a group as matrix effects use it. */
export function groupLayout(show: Show, group: FixtureGroup | undefined): GroupLayout {
  const json = group ? view(show).view.group_layout(group.id) : undefined;
  return json ? (JSON.parse(json) as GroupLayout) : { width: 1, height: 1, positions: [] };
}

export function colorToHex(c: Color): string {
  return linear_to_hex(c.r, c.g, c.b);
}

export function hexToColor(hex: string): Color {
  const [r = 0, g = 0, b = 0] = hex_to_linear(hex);
  return { r, g, b };
}

export interface NamedColor {
  name: string;
  /** Linear RGB. */
  color: Color;
}

/** Most colors `colorSequence` can return. */
export const maxSequenceColors = (): number => max_sequence_colors();

/** `n` bright, mutually distinct, named colors in color-wheel order. */
export function colorSequence(n: number): NamedColor[] {
  return JSON.parse(color_sequence_json(Math.max(0, Math.floor(n)))) as NamedColor[];
}

/** Beat lamp at a musical position (the engine's own function, also used for controller LEDs). */
export function beatLamp(beat: number, bpm: number, beatsPerBar: number): { on: boolean; down: boolean } {
  const bits = beat_flash_bits(beat, bpm, Math.max(1, Math.min(255, Math.round(beatsPerBar) || 1)));
  return { on: (bits & 1) !== 0, down: (bits & 2) !== 0 };
}

/**
 * Whether a widget is lit (the engine's own rule, also used for controller
 * LEDs): its function runs, or a step of a running chaser plays it.
 */
export function widgetLight(kind: WidgetKind, fn: number | null, blackout: boolean, active: ActiveFunction[]): 'off' | 'on' | 'paused' {
  const code = widget_light_code(JSON.stringify(kind), fn, blackout, JSON.stringify(active));
  return code === 1 ? 'on' : code === 2 ? 'paused' : 'off';
}

/** Types of function a chaser step can play. */
export const chaserStepTypes = (): FunctionCategory[] => JSON.parse(chaser_step_types()) as FunctionCategory[];

/** Beat lengths offered for chaser steps and fades and effect cycles. */
export const beatChoices = (): number[] => Array.from(beat_choices());

/** Movement effect geometry (degrees), from the engine's own functions. */
export class Movement {
  private v: MovementView;
  constructor(effect: MovementEffect) {
    this.v = new MovementView(JSON.stringify(effect));
  }
  path(samples = 180): Array<[number, number]> {
    return pairs(this.v.path(samples));
  }
  heads(turns: number, n: number): Array<[number, number]> {
    return pairs(this.v.heads(turns, Math.max(1, n)));
  }
  free() {
    this.v.free();
  }
}

/** Matrix effect colors (linear RGB per cell), from the engine's own functions. */
export class Matrix {
  private v: MatrixView;
  constructor(effect: MatrixEffect, layout: GroupLayout) {
    this.v = new MatrixView(JSON.stringify(effect), JSON.stringify(layout));
  }
  colors(turns: number): Color[] {
    const a = this.v.colors(turns);
    const out: Color[] = [];
    for (let i = 0; i + 2 < a.length; i += 3) out.push({ r: a[i]!, g: a[i + 1]!, b: a[i + 2]! });
    return out;
  }
  free() {
    this.v.free();
  }
}

function pairs(a: Float32Array): Array<[number, number]> {
  const out: Array<[number, number]> = [];
  for (let i = 0; i + 1 < a.length; i += 2) out.push([a[i]!, a[i + 1]!]);
  return out;
}
