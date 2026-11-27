<script lang="ts">
  // Pan (across) and tilt (up and down) picked by dragging, for finding a
  // position on stage. Fine mode, or Shift held, moves ten times slower, from
  // where the point is: the drag moves it, the point does not jump under the
  // pointer. Arrow keys step it (Shift: ten times smaller steps).
  let {
    pan,
    tilt,
    panRange,
    tiltRange,
    step = 1,
    unit = '°',
    onchange,
    oncommit,
  }: {
    pan: number;
    tilt: number;
    panRange: [number, number];
    tiltRange: [number, number];
    /** Smallest change the values take (e.g. 1 for DMX bytes). */
    step?: number;
    unit?: string;
    onchange: (pan: number, tilt: number) => void;
    /** The gesture ended (pointer let go, or an arrow key): the place to keep. */
    oncommit?: () => void;
  } = $props();

  let fine = $state(false);
  let pad = $state<HTMLElement>();
  let drag: { x: number; y: number; pan: number; tilt: number; slow: boolean } | null = null;

  const clamp = (v: number, [lo, hi]: [number, number]) => Math.min(hi, Math.max(lo, v));
  const snap = (v: number, slow: boolean) => {
    const s = slow ? step / 10 : step;
    return Math.round(v / s) * s;
  };
  const frac = (v: number, [lo, hi]: [number, number]) => (hi === lo ? 0.5 : (v - lo) / (hi - lo));
  const x = $derived(frac(clamp(pan, panRange), panRange) * 100);
  // Tilt up is at the top.
  const y = $derived((1 - frac(clamp(tilt, tiltRange), tiltRange)) * 100);

  function set(p: number, t: number, slow: boolean) {
    const np = clamp(snap(p, slow), panRange);
    const nt = clamp(snap(t, slow), tiltRange);
    if (np !== pan || nt !== tilt) onchange(np, nt);
  }

  function absolute(e: PointerEvent) {
    const r = pad?.getBoundingClientRect();
    if (!r || !r.width || !r.height) return;
    const fx = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const fy = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    set(panRange[0] + fx * (panRange[1] - panRange[0]), tiltRange[1] - fy * (tiltRange[1] - tiltRange[0]), false);
  }

  function down(e: PointerEvent) {
    const slow = fine || e.shiftKey;
    drag = { x: e.clientX, y: e.clientY, pan, tilt, slow };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    if (!slow) absolute(e);
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    if (!drag.slow) return absolute(e);
    const r = pad?.getBoundingClientRect();
    if (!r || !r.width || !r.height) return;
    const dp = ((e.clientX - drag.x) / r.width) * (panRange[1] - panRange[0]) / 10;
    const dt = (-(e.clientY - drag.y) / r.height) * (tiltRange[1] - tiltRange[0]) / 10;
    set(drag.pan + dp, drag.tilt + dt, true);
  }
  function up() {
    if (drag) oncommit?.();
    drag = null;
  }
  function key(e: KeyboardEvent) {
    const s = e.shiftKey || fine ? step / 10 : step;
    const moves: Record<string, [number, number]> = { ArrowLeft: [-s, 0], ArrowRight: [s, 0], ArrowUp: [0, s], ArrowDown: [0, -s] };
    const m = moves[e.key];
    if (!m) return;
    e.preventDefault();
    set(pan + m[0], tilt + m[1], e.shiftKey || fine);
  }
  function keyup(e: KeyboardEvent) {
    if (e.key.startsWith('Arrow')) oncommit?.();
  }
  const shown = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(1));
</script>

<div class="xy">
  <div
    class="pad"
    class:fine
    bind:this={pad}
    role="slider"
    tabindex="0"
    aria-label="Pan and tilt"
    aria-valuenow={pan}
    aria-valuetext="Pan {shown(pan)}{unit}, tilt {shown(tilt)}{unit}"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onkeydown={key}
    onkeyup={keyup}
  >
    <span class="axis h"></span>
    <span class="axis v"></span>
    <span class="dot" style="left:{x}%; top:{y}%"></span>
  </div>
  <div class="row">
    <span class="muted small">Pan {shown(pan)}{unit} · Tilt {shown(tilt)}{unit}</span>
    <label class="row small" title="Moves ten times slower (also with Shift held)"><input type="checkbox" bind:checked={fine} /> Fine</label>
  </div>
</div>

<style>
  .xy { display: flex; flex-direction: column; gap: 4px; }
  .pad { position: relative; aspect-ratio: 2 / 1; width: 100%; border: 1px solid var(--border); border-radius: var(--radius); background: var(--panel); touch-action: none; cursor: crosshair; }
  .pad:focus-visible { outline: 2px solid var(--accent); }
  .pad.fine { border-color: var(--accent); }
  .axis { position: absolute; background: var(--border); pointer-events: none; }
  .axis.h { left: 0; right: 0; top: 50%; height: 1px; }
  .axis.v { top: 0; bottom: 0; left: 50%; width: 1px; }
  .dot { position: absolute; width: 14px; height: 14px; margin: -7px 0 0 -7px; border-radius: 50%; background: var(--accent); border: 2px solid var(--text); pointer-events: none; }
  .small { font-size: 12px; }
  .row { justify-content: space-between; }
</style>
