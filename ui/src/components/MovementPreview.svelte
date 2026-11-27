<script lang="ts" module>
  /** Limits of the controls (degrees), shared with the editor's sliders. */
  export const LIMITS = { pan: 270, tilt: 135, width: 540, height: 270 } as const;
</script>

<script lang="ts">
  // Live drawing of a movement effect (pan/tilt in degrees), like QLC+. The
  // shape is edited on the drawing too: drag its center to move it, the
  // handles on its right and top edges to resize it (or focus them and use the
  // arrow keys, Shift for 10°). Where the lights reach is drawn around it.
  import { Movement } from '../lib/core';
  import type { MovementEffect, MovementReach } from '../lib/protocol';

  type Edit = Partial<Pick<MovementEffect, 'center_pan' | 'center_tilt' | 'width' | 'height'>>;

  let {
    efx,
    reach,
    bpm,
    onedit,
    oncommit,
  }: {
    efx: MovementEffect;
    reach: MovementReach;
    bpm: number;
    /** A value changed by dragging or with the keys (drawn at once). */
    onedit: (edit: Edit) => void;
    /** The drag ended or a key was pressed: send it to the engine. */
    oncommit: () => void;
  } = $props();

  let fullRange = $state(false);
  let turns = $state(0);

  // Animate the heads along the path. The cycles are counted from absolute
  // time, so editing values never restarts them (ping-pong needs the count,
  // not only the phase).
  const periodMs = $derived('ms' in efx.period ? efx.period.ms : (efx.period.beats * 60000) / Math.max(1, bpm));
  $effect(() => {
    let raf = 0;
    const loop = (now: number) => {
      turns = periodMs > 0 ? now / periodMs : 0;
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  // The engine's own geometry (WebAssembly); rebuilt when the effect changes.
  const geometry = $derived(new Movement(efx));
  $effect(() => {
    const g = geometry;
    return () => g.free();
  });
  const path = $derived(geometry.path());
  const anyMirrored = $derived(reach.heads.some((h) => h.mirrored));
  const mirroredPath = $derived(anyMirrored ? geometry.path(180, true) : []);
  const count = $derived(Math.max(1, reach.heads.length));

  // What the lights reach (a side without a range is not limited).
  const FAR = 1e4;
  const reachBox = $derived({
    pan: reach.pan_limit ?? FAR,
    tilt: reach.tilt_limit ?? FAR,
  });
  const fitted = $derived.by(() => {
    if (fullRange) {
      const hx = Math.max(reach.pan_limit ?? LIMITS.pan, (reach.tilt_limit ?? LIMITS.tilt) * 2) * 1.05;
      return { x: -hx, y: -hx / 2, w: hx * 2, h: hx };
    }
    const all = [...path, ...mirroredPath];
    const xs = all.map((p) => p[0]);
    const ys = all.map((p) => p[1]);
    // Same 2:1 aspect as the drawing area, with a margin around the shape.
    const hx = Math.max(15, (Math.max(...xs) - Math.min(...xs)) * 0.6, (Math.max(...ys) - Math.min(...ys)) * 1.2);
    const hy = hx / 2;
    return { x: efx.center_pan - hx, y: efx.center_tilt - hy, w: hx * 2, h: hy * 2 };
  });
  // While dragging, the view stays still so the shape moves under the pointer.
  let frozen = $state<typeof fitted | null>(null);
  const box = $derived(frozen ?? fitted);

  // SVG y grows downwards; tilt up is drawn up.
  const line = (points: Array<[number, number]>) => points.map(([p, t], i) => `${i ? 'L' : 'M'}${p.toFixed(2)},${(-t).toFixed(2)}`).join(' ');
  const d = $derived(line(path));
  const dMirrored = $derived(line(mirroredPath));
  const dots = $derived(geometry.heads(turns, count));
  const stroke = $derived(box.w / 300);
  const ticks = $derived.by(() => {
    const step = box.w > 300 ? 90 : box.w > 120 ? 30 : 10;
    const out: number[] = [];
    for (let v = Math.ceil(box.x / step) * step; v <= box.x + box.w; v += step) out.push(v);
    return { step, xs: out };
  });
  /** Outside what the lights reach: shaded (even-odd: the box minus the reach). */
  const outside = $derived.by(() => {
    if (reach.pan_limit === null && reach.tilt_limit === null) return null;
    const [x0, x1, y0, y1] = [box.x, box.x + box.w, -(box.y + box.h), -box.y];
    const { pan: p, tilt: t } = reachBox;
    return `M${x0},${y0}H${x1}V${y1}H${x0}Z M${-p},${-t}H${p}V${t}H${-p}Z`;
  });
  const showNumbers = $derived(count <= 24);
  const hasHeight = $derived(efx.shape.type !== 'line');

  // ---- dragging
  type Part = 'center' | 'width' | 'height';
  let svg = $state<SVGSVGElement>();
  let dragging: { part: Part; dx: number; dy: number; id: number } | null = null;

  const round = (v: number) => Math.round(v * 2) / 2;
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  /** Pointer position in degrees (pan, tilt). */
  function degrees(e: PointerEvent): [number, number] | null {
    const m = svg?.getScreenCTM();
    if (!svg || !m) return null;
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return [p.x, -p.y];
  }

  function grab(part: Part, e: PointerEvent) {
    const at = degrees(e);
    if (!at || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    frozen = { ...fitted };
    // Where in the handle it was grabbed, so it does not jump.
    const [hp, ht] = handle(part);
    dragging = { part, dx: at[0] - hp, dy: at[1] - ht, id: e.pointerId };
  }

  function handle(part: Part): [number, number] {
    if (part === 'width') return [efx.center_pan + efx.width / 2, efx.center_tilt];
    if (part === 'height') return [efx.center_pan, efx.center_tilt + efx.height / 2];
    return [efx.center_pan, efx.center_tilt];
  }

  function drag(e: PointerEvent) {
    if (!dragging || e.pointerId !== dragging.id) return;
    const at = degrees(e);
    if (!at) return;
    const [pan, tilt] = [at[0] - dragging.dx, at[1] - dragging.dy];
    if (dragging.part === 'center') {
      onedit({ center_pan: round(clamp(pan, -LIMITS.pan, LIMITS.pan)), center_tilt: round(clamp(tilt, -LIMITS.tilt, LIMITS.tilt)) });
    } else if (dragging.part === 'width') {
      onedit({ width: round(clamp(2 * Math.abs(pan - efx.center_pan), 0, LIMITS.width)) });
    } else {
      onedit({ height: round(clamp(2 * Math.abs(tilt - efx.center_tilt), 0, LIMITS.height)) });
    }
  }

  function release(e: PointerEvent) {
    if (!dragging || e.pointerId !== dragging.id) return;
    dragging = null;
    frozen = null;
    oncommit();
  }

  function key(part: Part, e: KeyboardEvent) {
    const step = e.shiftKey ? 10 : 1;
    const dir = ({ ArrowRight: [1, 0], ArrowLeft: [-1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] } as Record<string, [number, number]>)[e.key];
    if (!dir) return;
    e.preventDefault();
    const [x, y] = [dir[0] * step, dir[1] * step];
    if (part === 'center') {
      onedit({ center_pan: clamp(efx.center_pan + x, -LIMITS.pan, LIMITS.pan), center_tilt: clamp(efx.center_tilt + y, -LIMITS.tilt, LIMITS.tilt) });
    } else if (part === 'width') {
      onedit({ width: clamp(efx.width + 2 * (x || y), 0, LIMITS.width) });
    } else {
      onedit({ height: clamp(efx.height + 2 * (y || x), 0, LIMITS.height) });
    }
    oncommit();
  }
</script>

<div class="preview">
  <svg
    bind:this={svg}
    viewBox="{box.x} {-(box.y + box.h)} {box.w} {box.h}"
    preserveAspectRatio="xMidYMid meet"
    role="img"
    aria-label="Movement preview"
    onpointermove={drag}
    onpointerup={release}
    onpointercancel={release}
  >
    {#each ticks.xs as v (v)}
      <line x1={v} y1={-(box.y + box.h)} x2={v} y2={-box.y} class="grid" stroke-width={stroke / 2} />
      <text x={v} y={-box.y - stroke * 2} font-size={stroke * 8} class="label">{v}°</text>
    {/each}
    <line x1={box.x} y1="0" x2={box.x + box.w} y2="0" class="axis" stroke-width={stroke / 1.5} />
    <line x1="0" y1={-(box.y + box.h)} x2="0" y2={-box.y} class="axis" stroke-width={stroke / 1.5} />
    {#if outside}
      <path d={outside} class="unreachable" fill-rule="evenodd" data-reach />
    {/if}
    {#if anyMirrored}
      <path d={dMirrored} class="shape mirrored" class:clipped={reach.clipped} stroke-width={stroke} fill="none" data-mirrored />
    {/if}
    <path {d} class="shape" class:clipped={reach.clipped} stroke-width={stroke * 1.5} fill="none" />
    {#each dots as [p, t], i (i)}
      {#if reach.heads[i]?.moves ?? true}
        <g class="head" class:first={i === 0}>
          <circle cx={p} cy={-t} r={stroke * 5} />
          {#if showNumbers}<text x={p} y={-t} font-size={stroke * 6} dy="0.35em">{i + 1}</text>{/if}
          <title>{i + 1}. {reach.heads[i]?.name ?? ''}</title>
        </g>
      {/if}
    {/each}
    <!-- Handles: size on the right and top edges, the center in the middle. -->
    <g class="handle" role="button" tabindex="0" aria-label="Width: drag or use the arrow keys"
      onpointerdown={(e) => grab('width', e)} onkeydown={(e) => key('width', e)}>
      <rect x={efx.center_pan + efx.width / 2 - stroke * 3} y={-efx.center_tilt - stroke * 6} width={stroke * 6} height={stroke * 12} rx={stroke} />
    </g>
    {#if hasHeight}
      <g class="handle" role="button" tabindex="0" aria-label="Height: drag or use the arrow keys"
        onpointerdown={(e) => grab('height', e)} onkeydown={(e) => key('height', e)}>
        <rect x={efx.center_pan - stroke * 6} y={-(efx.center_tilt + efx.height / 2) - stroke * 3} width={stroke * 12} height={stroke * 6} rx={stroke} />
      </g>
    {/if}
    <g class="handle center" role="button" tabindex="0" aria-label="Center: drag or use the arrow keys"
      onpointerdown={(e) => grab('center', e)} onkeydown={(e) => key('center', e)}>
      <circle cx={efx.center_pan} cy={-efx.center_tilt} r={stroke * 5} />
      <path d="M{efx.center_pan - stroke * 3},{-efx.center_tilt}h{stroke * 6}M{efx.center_pan},{-efx.center_tilt - stroke * 3}v{stroke * 6}" stroke-width={stroke} />
    </g>
  </svg>
  <div class="row legend">
    <span class="muted">
      Drag the center or the edges · pan horizontal, tilt vertical (degrees){#if efx.relative} · relative: drawn as if the scenes left the heads in the middle{/if}{#if anyMirrored} · dashed: the mirrored heads{/if}
    </span>
    <label class="row"><input type="checkbox" bind:checked={fullRange} /> Full range</label>
  </div>
</div>

<style>
  .preview { background: var(--panel-2); border: 1px solid var(--border); border-radius: var(--radius); padding: 6px; }
  svg { width: 100%; aspect-ratio: 2 / 1; display: block; touch-action: none; user-select: none; }
  .grid { stroke: var(--border); }
  .axis { stroke: var(--muted); }
  .label { fill: var(--muted); text-anchor: middle; }
  .unreachable { fill: color-mix(in srgb, var(--danger, #e5484d) 14%, transparent); stroke: color-mix(in srgb, var(--danger, #e5484d) 60%, transparent); stroke-dasharray: 2 2; stroke-width: 0.3; vector-effect: non-scaling-stroke; }
  .shape { stroke: var(--accent); }
  .shape.clipped { stroke: var(--warn); }
  .shape.mirrored { stroke-dasharray: 3 2; opacity: 0.7; vector-effect: non-scaling-stroke; stroke-width: 1.5; }
  .head circle { fill: var(--text); opacity: 0.75; }
  .head.first circle { fill: var(--ok); opacity: 1; }
  .head text { fill: var(--bg, #000); text-anchor: middle; font-weight: 700; pointer-events: none; }
  .handle { cursor: grab; outline: none; }
  .handle rect, .handle circle { fill: var(--panel); stroke: var(--warn); stroke-width: 0.4; vector-effect: non-scaling-stroke; }
  .handle path { stroke: var(--warn); }
  .handle:focus-visible rect, .handle:focus-visible circle { stroke: var(--accent); stroke-width: 2; }
  .handle.center { cursor: move; }
  .legend { justify-content: space-between; font-size: 12px; }
</style>
