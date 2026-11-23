<script lang="ts">
  // Live drawing of a movement effect (pan/tilt in degrees), like QLC+.
  import { Movement } from '../lib/core';
  import type { MovementEffect } from '../lib/protocol';

  let { efx, heads, bpm }: { efx: MovementEffect; heads: number; bpm: number } = $props();

  let fullRange = $state(false);
  let phase = $state(0);

  // Animate the heads along the path. The phase comes from absolute time,
  // so editing values never restarts the cycle.
  const periodMs = $derived('ms' in efx.period ? efx.period.ms : (efx.period.beats * 60000) / Math.max(1, bpm));
  $effect(() => {
    let raf = 0;
    const loop = (now: number) => {
      phase = periodMs > 0 ? (now % periodMs) / periodMs : 0;
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
  const box = $derived.by(() => {
    if (fullRange) return { x: -270, y: -135, w: 540, h: 270 };
    const xs = path.map((p) => p[0]);
    const ys = path.map((p) => p[1]);
    // Same 2:1 aspect as the drawing area, with a margin around the shape.
    const hx = Math.max(15, (Math.max(...xs) - Math.min(...xs)) * 0.6, (Math.max(...ys) - Math.min(...ys)) * 1.2);
    const hy = hx / 2;
    return { x: efx.center_pan - hx, y: efx.center_tilt - hy, w: hx * 2, h: hy * 2 };
  });
  // SVG y grows downwards; tilt up is drawn up.
  const d = $derived(path.map(([p, t], i) => `${i ? 'L' : 'M'}${p.toFixed(2)},${(-t).toFixed(2)}`).join(' '));
  const dots = $derived(geometry.heads(phase, heads));
  const stroke = $derived(box.w / 300);
  const ticks = $derived.by(() => {
    const step = box.w > 300 ? 90 : box.w > 120 ? 30 : 10;
    const out: number[] = [];
    for (let v = Math.ceil(box.x / step) * step; v <= box.x + box.w; v += step) out.push(v);
    return { step, xs: out };
  });
</script>

<div class="preview">
  <svg viewBox="{box.x} {-(box.y + box.h)} {box.w} {box.h}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="Movement preview">
    {#each ticks.xs as v (v)}
      <line x1={v} y1={-(box.y + box.h)} x2={v} y2={-box.y} class="grid" stroke-width={stroke / 2} />
      <text x={v} y={-box.y - stroke * 2} font-size={stroke * 8} class="label">{v}°</text>
    {/each}
    <line x1={box.x} y1="0" x2={box.x + box.w} y2="0" class="axis" stroke-width={stroke / 1.5} />
    <line x1="0" y1={-(box.y + box.h)} x2="0" y2={-box.y} class="axis" stroke-width={stroke / 1.5} />
    <path {d} class="shape" stroke-width={stroke * 1.5} fill="none" />
    <circle cx={efx.center_pan} cy={-efx.center_tilt} r={stroke * 2} class="center" />
    {#each dots as [p, t], i (i)}
      <circle cx={p} cy={-t} r={stroke * 4} class="head" class:first={i === 0} />
    {/each}
  </svg>
  <div class="row legend">
    <span class="muted">Pan horizontal, tilt vertical (degrees). {heads} head{heads === 1 ? '' : 's'}.</span>
    <label class="row"><input type="checkbox" bind:checked={fullRange} /> Full range</label>
  </div>
</div>

<style>
  .preview { background: var(--panel-2); border: 1px solid var(--border); border-radius: var(--radius); padding: 6px; }
  svg { width: 100%; aspect-ratio: 2 / 1; display: block; }
  .grid { stroke: var(--border); }
  .axis { stroke: var(--muted); }
  .label { fill: var(--muted); text-anchor: middle; }
  .shape { stroke: var(--accent); }
  .center { fill: var(--warn); }
  .head { fill: var(--text); opacity: 0.8; }
  .head.first { fill: var(--ok); opacity: 1; }
  .legend { justify-content: space-between; font-size: 12px; }
</style>
