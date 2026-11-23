<script lang="ts">
  import { untrack } from 'svelte';
  // Movement effect editor: the drawing follows the values while dragging;
  // the engine receives them when the control is released.
  import { app, run } from '../lib/app.svelte';
  import { formatTime, parseTime } from '../lib/show';
  import type { Function as Fn, MovementEffect, MovementShape } from '../lib/protocol';
  import MovementPreview from './MovementPreview.svelte';

  let { f }: { f: Fn } = $props();
  const show = $derived(app.s.show!);
  const source = $derived(f.kind.type === 'movement_effect' ? f.kind : null);
  let draft = $state<MovementEffect | null>(null);

  // Follow the engine's version (after every commit or remote edit).
  /** JSON of the last state known to match the engine. */
  let synced = '';
  let syncedId = -1;
  // Follow the engine's version, but never overwrite local edits that have
  // not been sent yet (a late confirmation of an earlier change must not
  // reset what the operator is typing).
  $effect(() => {
    const src = source;
    const id = f.id;
    if (!src) return;
    const { type: _type, ...rest } = src;
    const incoming = $state.snapshot(rest) as MovementEffect;
    untrack(() => {
      const local = draft ? JSON.stringify($state.snapshot(draft)) : null;
      if (id !== syncedId || local === null || local === synced) draft = incoming;
      synced = JSON.stringify(incoming);
      syncedId = id;
    });
  });

  const heads = $derived(show.groups.find((g) => g.id === draft?.group)?.members.length ?? 1);

  function commit() {
    if (!draft) return;
    synced = JSON.stringify($state.snapshot(draft));
    run({ type: 'set_function', function: { ...f, kind: { type: 'movement_effect', ...draft } } });
  }

  function setShape(type: string) {
    if (!draft) return;
    const shapes: Record<string, MovementShape> = {
      circle: { type: 'circle' },
      eight: { type: 'eight' },
      line: { type: 'line' },
      square: { type: 'square' },
      lissajous: { type: 'lissajous', x_freq: 3, y_freq: 2 },
    };
    draft.shape = shapes[type] ?? { type: 'circle' };
    commit();
  }

  const sliders = [
    { key: 'center_pan', label: 'Center pan', min: -270, max: 270 },
    { key: 'center_tilt', label: 'Center tilt', min: -135, max: 135 },
    { key: 'width', label: 'Width', min: 0, max: 540 },
    { key: 'height', label: 'Height', min: 0, max: 270 },
    { key: 'phase_spread', label: 'Phase spread', min: 0, max: 360 },
  ] as const;
</script>

{#if draft}
  <div class="editor">
    <MovementPreview efx={draft} {heads} bpm={app.s.runtime?.state.tempo.bpm ?? 120} />
    <div class="col">
      <label class="row">Group
        <select value={draft.group} onchange={(e) => { draft!.group = Number(e.currentTarget.value); commit(); }}>
          {#each show.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
        </select>
      </label>
      <div class="row shapes">
        {#each ['circle', 'eight', 'line', 'square', 'lissajous'] as s (s)}
          <button class:active={draft.shape.type === s} onclick={() => setShape(s)}>{s}</button>
        {/each}
      </div>
      {#if draft.shape.type === 'lissajous'}
        {@const shape = draft.shape}
        <div class="row">
          <label>X freq <input type="number" min="1" max="12" value={shape.x_freq} style="width:60px" onchange={(e) => { draft!.shape = { ...shape, x_freq: Number(e.currentTarget.value) }; commit(); }} /></label>
          <label>Y freq <input type="number" min="1" max="12" value={shape.y_freq} style="width:60px" onchange={(e) => { draft!.shape = { ...shape, y_freq: Number(e.currentTarget.value) }; commit(); }} /></label>
        </div>
      {/if}
      {#each sliders as s (s.key)}
        <label class="slider">
          <span>{s.label}</span>
          <input type="range" min={s.min} max={s.max} step="0.5" bind:value={draft[s.key]} onchange={commit} />
          <input type="number" min={s.min} max={s.max} step="0.5" bind:value={draft[s.key]} onchange={commit} class="num" />°
        </label>
      {/each}
      <div class="row">
        <button onclick={() => {
          const text = window.prompt('Cycle duration (e.g. 2000, 4s, 4 beats)', formatTime(draft!.period));
          const t = text === null ? null : parseTime(text);
          if (t) { draft!.period = t; commit(); }
        }}>Cycle: {formatTime(draft.period)}</button>
        <button onclick={() => { draft!.direction = draft!.direction === 'Forward' ? 'Backward' : 'Forward'; commit(); }}>
          Direction: {draft.direction}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .editor { display: grid; grid-template-columns: minmax(280px, 1.2fr) 1fr; gap: 12px; align-items: start; }
  @media (max-width: 1100px) { .editor { grid-template-columns: 1fr; } }
  .slider { display: grid; grid-template-columns: 100px 1fr 72px auto; align-items: center; gap: 6px; }
  .num { width: 72px; }
  .shapes button { text-transform: capitalize; }
</style>
