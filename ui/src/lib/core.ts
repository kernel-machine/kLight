// Core logic in the browser: the engine's own Rust functions compiled to
// WebAssembly (crates/klight-wasm). The UI never re-implements them, so
// previews and patch checks match the engine by construction.
//
// Call `initCore()` once before using anything else.

import init, { initSync, ShowView, MovementView, MatrixView, linear_to_hex, hex_to_linear } from './wasm/klight_wasm.js';
import type {
  ChannelInfo,
  Color,
  FixtureGroup,
  FixtureInfo,
  FixturePreview,
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
  previews: Map<number, FixturePreview[]>;
} | null = null;

function view(show: Show) {
  if (cached?.show !== show) {
    cached?.view.free();
    cached = { show, view: new ShowView(JSON.stringify(show)), channels: new Map(), infos: new Map(), previews: new Map() };
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

/** What a scene sets on each fixture and channel (engine's own compilation). */
export function scenePreview(show: Show, functionId: number): FixturePreview[] {
  const c = view(show);
  let p = c.previews.get(functionId);
  if (!p) {
    const json = c.view.scene_preview(functionId);
    p = json ? (JSON.parse(json) as FixturePreview[]) : [];
    c.previews.set(functionId, p);
  }
  return p;
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
