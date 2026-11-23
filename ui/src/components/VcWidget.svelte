<script lang="ts">
  import { app, run } from '../lib/app.svelte';
  import type { Widget } from '../lib/protocol';
  import VcWidget from './VcWidget.svelte';
  import Knob from './Knob.svelte';
  import { createBeatLamp } from '../lib/beatLamp.svelte';

  let {
    widget,
    design,
    selected,
    onselect,
    onmove,
  }: {
    widget: Widget;
    design: boolean;
    selected: number | null;
    onselect: (id: number) => void;
    onmove: (id: number, dx: number, dy: number) => void;
  } = $props();

  const p = $derived(widget.props);
  const active = $derived(app.s.runtime?.state.active.find((a) => a.id === widget.function));
  const isOn = $derived(
    widget.kind.type === 'button' && widget.kind.action === 'Blackout' ? !!app.s.runtime?.state.blackout : !!active,
  );
  const color = $derived(p.color ? `rgb(${p.color.r},${p.color.g},${p.color.b})` : 'var(--accent)');
  const level = $derived(
    widget.kind.type === 'slider' && widget.kind.mode === 'GrandMaster' ? (app.s.runtime?.state.grand_master ?? 1) : (active?.level ?? 0),
  );

  // `new Date()` is not reactive, so the clock needs its own ticking state.
  let now = $state(new Date());
  $effect(() => {
    if (widget.kind.type !== 'clock') return;
    now = new Date();
    const timer = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(timer);
  });

  // Tap buttons and the speed dial blink on the beat.
  const isTap = $derived((widget.kind.type === 'button' && widget.kind.action === 'TapTempo') || widget.kind.type === 'speed_dial');
  const beat = createBeatLamp(() => isTap);

  let drag: { x: number; y: number } | null = null;
  let offset = $state({ x: 0, y: 0 });

  function down(e: PointerEvent) {
    if (design) {
      e.stopPropagation();
      onselect(widget.id);
      drag = { x: e.clientX, y: e.clientY };
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      return;
    }
    if (widget.kind.type === 'button' || widget.kind.type === 'cue_list' || widget.kind.type === 'speed_dial') {
      run({ type: 'widget_press', id: widget.id });
    }
  }
  function move(e: PointerEvent) {
    if (drag) offset = { x: e.clientX - drag.x, y: e.clientY - drag.y };
  }
  // After a drop the widget keeps its (snapped) offset until the new position
  // arrives from the engine; resetting it immediately would flash the widget
  // back to its old position for one round trip.
  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void p.x;
    void p.y;
    clearTimeout(settleTimer);
    if (!drag) offset = { x: 0, y: 0 };
  });
  function up() {
    if (design && drag) {
      const dx = Math.round(offset.x / 10) * 10;
      const dy = Math.round(offset.y / 10) * 10;
      drag = null;
      if (dx || dy) {
        offset = { x: dx, y: dy };
        onmove(widget.id, dx, dy);
        // Fallback if the move is rejected and the position never changes.
        settleTimer = setTimeout(() => (offset = { x: 0, y: 0 }), 1500);
      } else {
        offset = { x: 0, y: 0 };
      }
      return;
    }
    if (!design && widget.kind.type === 'button' && widget.kind.action === 'Flash') run({ type: 'widget_release', id: widget.id });
  }
</script>

<div
  class="w {widget.kind.type}"
  class:sel={design && selected === widget.id}
  class:on={isOn}
  class:beat={beat.on}
  class:down={beat.on && beat.down}
  style="left:{p.x + offset.x}px; top:{p.y + offset.y}px; width:{p.width}px; height:{p.height}px; --c:{color}"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  role="button"
  tabindex="0"
>
  {#if widget.kind.type === 'frame' || widget.kind.type === 'solo_frame'}
    <div class="label">{p.label}{widget.kind.type === 'solo_frame' ? ' (solo)' : ''}</div>
    {#each widget.children as c (c.id)}
      <VcWidget widget={c} {design} {selected} {onselect} {onmove} />
    {/each}
  {:else if widget.kind.type === 'knob'}
    <div class="label">{p.label}</div>
    <Knob value={level} disabled={design} label={p.label} onchange={(v) => run({ type: 'widget_set_value', id: widget.id, value: v })} />
    <div class="val">{Math.round(level * 100)}%</div>
  {:else if widget.kind.type === 'slider'}
    <div class="label">{p.label}</div>
    <input
      type="range"
      min="0"
      max="1"
      step="0.005"
      value={level}
      disabled={design}
      class="vertical"
      oninput={(e) => run({ type: 'widget_set_value', id: widget.id, value: Number(e.currentTarget.value) })}
    />
    <div class="val">{Math.round(level * 100)}%</div>
  {:else if widget.kind.type === 'cue_list'}
    <div class="label">{p.label}</div>
    <div class="big">GO</div>
    <div class="val">{active?.position !== undefined && active?.position !== null ? `Cue ${active.position + 1}` : '—'}</div>
  {:else if widget.kind.type === 'clock'}
    <div class="label">{p.label}</div>
    <div class="big">{now.toLocaleTimeString()}</div>
  {:else if widget.kind.type === 'speed_dial'}
    <div class="label">{p.label}</div>
    <div class="big">{app.s.runtime?.state.tempo.bpm.toFixed(1)}</div>
    <div class="val">tap</div>
  {:else}
    <div class="label">{p.label}</div>
  {/if}
</div>

<style>
  .w { position: absolute; border: 1px solid var(--border); border-radius: var(--radius); background: var(--panel-2); display: flex; flex-direction: column; align-items: center; justify-content: center; user-select: none; overflow: hidden; touch-action: none; }
  .w.button, .w.cue_list, .w.speed_dial { cursor: pointer; border-color: var(--c); }
  .w.button.on { background: var(--c); color: #000; font-weight: 700; }
  .w.beat { background: color-mix(in srgb, var(--c) 55%, var(--panel-2)); }
  .w.beat.down { background: var(--c); color: #000; }
  .w.frame, .w.solo_frame { justify-content: flex-start; background: color-mix(in srgb, var(--panel) 80%, transparent); }
  .w.label { border: none; background: transparent; }
  .sel { outline: 2px dashed var(--warn); }
  .label { font-size: 12px; padding: 2px 4px; text-align: center; }
  .big { font-size: 22px; font-weight: 700; }
  .val { font-size: 11px; color: var(--muted); }
  .vertical { writing-mode: vertical-lr; direction: rtl; flex: 1; width: 32px; }
</style>
