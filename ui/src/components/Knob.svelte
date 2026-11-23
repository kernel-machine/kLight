<script lang="ts">
  // Rotary knob: drag up/down (or around), mouse wheel, arrow keys.
  let {
    value,
    disabled = false,
    label = '',
    onchange,
  }: { value: number; disabled?: boolean; label?: string; onchange: (v: number) => void } = $props();

  const START = -135; // degrees, 0 = up
  const SWEEP = 270;
  let local = $state<number | null>(null);
  const shown = $derived(local ?? value);
  let drag: { y: number; v: number } | null = null;
  let pending: number | null = null;
  let raf = 0;

  /** Sends at most one value per animation frame. */
  function emit(v: number) {
    const c = Math.min(1, Math.max(0, v));
    local = c;
    pending = c;
    if (!raf) {
      raf = requestAnimationFrame(() => {
        raf = 0;
        if (pending !== null) onchange(pending);
        pending = null;
      });
    }
  }

  function down(e: PointerEvent) {
    if (disabled) return;
    e.stopPropagation();
    drag = { y: e.clientY, v: shown };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    // 200 px of vertical travel = full range; shift for fine control.
    const range = e.shiftKey ? 1000 : 200;
    emit(drag.v + (drag.y - e.clientY) / range);
  }
  function up() {
    drag = null;
    local = null;
  }
  function wheel(e: WheelEvent) {
    if (disabled) return;
    e.preventDefault();
    emit(shown - Math.sign(e.deltaY) * 0.02);
    local = null;
  }
  function key(e: KeyboardEvent) {
    if (disabled) return;
    const step = e.shiftKey ? 0.1 : 0.01;
    if (e.key === 'ArrowUp' || e.key === 'ArrowRight') emit(shown + step);
    else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') emit(shown - step);
    else return;
    e.preventDefault();
    local = null;
  }

  function polar(angle: number, r: number): [number, number] {
    const a = ((angle - 90) * Math.PI) / 180;
    return [50 + r * Math.cos(a), 50 + r * Math.sin(a)];
  }
  function arc(from: number, to: number, r: number): string {
    const [x1, y1] = polar(from, r);
    const [x2, y2] = polar(to, r);
    const large = to - from > 180 ? 1 : 0;
    return `M${x1} ${y1} A${r} ${r} 0 ${large} 1 ${x2} ${y2}`;
  }
  const angle = $derived(START + shown * SWEEP);
  const tip = $derived(polar(angle, 30));
</script>

<div
  class="knob"
  class:disabled
  role="slider"
  tabindex={disabled ? -1 : 0}
  aria-label={label || 'Knob'}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={Math.round(shown * 100)}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onwheel={wheel}
  onkeydown={key}
>
  <svg viewBox="0 0 100 100">
    <path d={arc(START, START + SWEEP, 42)} class="track" />
    {#if shown > 0.001}<path d={arc(START, angle, 42)} class="fill" />{/if}
    <circle cx="50" cy="50" r="32" class="cap" />
    <line x1="50" y1="50" x2={tip[0]} y2={tip[1]} class="pointer" />
  </svg>
</div>

<style>
  .knob { width: 100%; max-width: 90px; aspect-ratio: 1; cursor: ns-resize; touch-action: none; outline: none; }
  .knob:focus-visible { outline: 2px solid var(--accent); border-radius: 50%; }
  .knob.disabled { cursor: default; opacity: 0.6; }
  svg { width: 100%; height: 100%; display: block; }
  .track { fill: none; stroke: var(--border); stroke-width: 8; stroke-linecap: round; }
  .fill { fill: none; stroke: var(--c, var(--accent)); stroke-width: 8; stroke-linecap: round; }
  .cap { fill: var(--panel); stroke: var(--border); stroke-width: 2; }
  .pointer { stroke: var(--text); stroke-width: 5; stroke-linecap: round; }
</style>
