<script lang="ts">
  // Movement effect editor, laid out as the scene and chaser editors: a header
  // (name, Start/Stop to try it on the lights, rare actions in a menu), the
  // drawing (edited directly: drag the center and the edges), where the lights
  // reach and which head is which, then the controls. The drawing follows the
  // values while dragging; the engine receives them when the control is
  // released. Edits and renames go through one queue, each applied to the
  // latest version of the function.
  import { untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { fxDraft } from '../lib/fxDraft.svelte';
  import { movementReach } from '../lib/core';
  import FxHeader from './FxHeader.svelte';
  import FxTiming from './FxTiming.svelte';
  import type { Function as Fn, MovementEffect, MovementMirror, MovementShape } from '../lib/protocol';
  import MovementPreview, { LIMITS } from './MovementPreview.svelte';

  let { f, onduplicate, ondelete }: { f: Fn; onduplicate: () => void; ondelete: () => void } = $props();
  const show = $derived(app.s.show!);
  const source = $derived(f.kind.type === 'movement_effect' ? f.kind : null);
  let draft = $state<MovementEffect | null>(null);

  // Follow the engine's version without undoing edits still on their way.
  const fx = fxDraft<MovementEffect>('movement_effect', () => f, () => draft, (d) => (draft = d));
  const { commit, rename } = fx;
  $effect(() => {
    const incoming = source ? fx.strip(source) : null;
    void f.id;
    untrack(() => fx.follow(incoming));
  });

  // Heads, ranges and clipping from the core, for the draft as it is drawn.
  const reach = $derived(draft ? movementReach(show, $state.snapshot(draft) as MovementEffect) : null);
  const moving = $derived(reach?.heads.filter((h) => h.moves).length ?? 0);

  const SHAPES: Array<[MovementShape['type'], string]> = [
    ['circle', 'Circle'],
    ['eight', 'Eight'],
    ['line', 'Line'],
    ['polygon', 'Polygon'],
    ['sine', 'Sine wave'],
    ['fan', 'Fan (opens and closes)'],
    ['lissajous', 'Lissajous'],
  ];

  function setShape(type: string) {
    if (!draft) return;
    const shapes: Record<string, MovementShape> = {
      circle: { type: 'circle' },
      eight: { type: 'eight' },
      line: { type: 'line' },
      polygon: { type: 'polygon', sides: 4 },
      sine: { type: 'sine', periods: 2 },
      fan: { type: 'fan', hold: 10 },
      lissajous: { type: 'lissajous', x_freq: 3, y_freq: 2 },
    };
    draft.shape = shapes[type] ?? { type: 'circle' };
    // A fan opens its heads to opposite sides by their place in the group:
    // together, and without mirroring, is how it reads.
    if (type === 'fan') {
      draft.phase_spread = 0;
      draft.mirror = 'none';
    }
    commit();
  }

  /** A parameter of the current shape, kept in the draft until the slider is released. */
  function setShapeParam(key: string, value: number) {
    if (draft) draft.shape = { ...draft.shape, [key]: value } as MovementShape;
  }

  // A relative shape is placed by an offset from where the scenes put the heads.
  const sliders = $derived([
    { key: 'center_pan', label: draft?.relative ? 'Offset pan' : 'Center pan', min: -LIMITS.pan, max: LIMITS.pan },
    { key: 'center_tilt', label: draft?.relative ? 'Offset tilt' : 'Center tilt', min: -LIMITS.tilt, max: LIMITS.tilt },
    { key: 'width', label: draft?.shape.type === 'fan' ? 'Opening' : 'Width', min: 0, max: LIMITS.width },
    { key: 'height', label: 'Height', min: 0, max: LIMITS.height },
    { key: 'rotation', label: 'Rotation', min: -180, max: 180 },
  ] as const);

  const MIRRORS: Array<[MovementMirror, string]> = [['none', 'None'], ['alternate', 'Every second head'], ['halves', 'Second half of the group']];

  /** Common ways to share the cycle among the heads (phase spread, degrees). */
  const PHASES = [
    { label: 'Together', value: 0, title: 'Every head at the same point of the shape' },
    { label: 'Wave', value: 180, title: 'The heads follow each other over half of the shape' },
    { label: 'Even', value: 360, title: 'The heads spaced evenly along the whole shape' },
  ] as const;
  function setPhase(value: number) {
    if (!draft) return;
    draft.phase_spread = value;
    commit();
  }

  const range = (limit: [number, number] | null) => {
    if (limit === null) return null;
    const [lo, hi] = limit.map(Math.round);
    return lo === -hi ? `±${hi}°` : `${lo}° to ${hi}°`;
  };
</script>

<FxHeader {f} badge="Movement" nameLabel="Movement name" deleteLabel="Delete movement" onrename={rename} {onduplicate} {ondelete} />

{#if draft && reach}
  <div class="editor">
    <div class="col fx-preview">
      <MovementPreview
        efx={draft}
        {reach}
        bpm={app.s.runtime?.state.tempo.bpm ?? 120}
        onedit={(edit) => Object.assign(draft!, edit)}
        oncommit={commit}
      />
      {#if !reach.heads.length}
        <p class="notice">This group has no lights.</p>
      {:else if !moving}
        <p class="notice">None of the lights in this group can move: they have no pan or tilt.</p>
      {:else if reach.clipped}
        <p class="notice warn" role="status">
          Part of the shape is beyond what the lights reach
          (pan {range(reach.pan_limit) ?? 'any'}, tilt {range(reach.tilt_limit) ?? 'any'}): there they stop at their limit.
        </p>
      {:else if reach.pan_limit !== null || reach.tilt_limit !== null}
        <p class="muted small">The lights reach pan {range(reach.pan_limit) ?? 'any'}, tilt {range(reach.tilt_limit) ?? 'any'}; the shaded area is out of reach.</p>
      {/if}
      {#if reach.heads.length}
        <ol class="heads" aria-label="Heads in order">
          {#each reach.heads as h, i (i)}
            <li class:still={!h.moves}><b>{i + 1}</b> {h.name}{#if h.mirrored}<span class="muted" title="Mirrored"> ⇄</span>{/if}{#if !h.moves}<span class="muted"> · no pan/tilt</span>{/if}</li>
          {/each}
        </ol>
      {/if}
    </div>

    <div class="fx-sections">
      <section class="fx-card">
        <h3>Lights</h3>
        <label class="fx-field">
          <span>Group</span>
          <select value={draft.group} aria-label="Movement group" onchange={(e) => { draft!.group = Number(e.currentTarget.value); commit(); }}>
            {#each show.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
          </select>
        </label>
        <label class="check" title="The shape is added to the position the scenes give each head; the center becomes an offset from there">
          <input type="checkbox" checked={draft.relative} onchange={(e) => { draft!.relative = e.currentTarget.checked; commit(); }} />
          Relative to the position of the scenes
        </label>
      </section>

      <section class="fx-card">
        <h3>Shape</h3>
        <label class="fx-field">
          <span>Type</span>
          <select aria-label="Shape" value={draft.shape.type} onchange={(e) => setShape(e.currentTarget.value)}>
            {#each SHAPES as [value, label] (value)}<option {value}>{label}</option>{/each}
          </select>
        </label>
        {#if draft.shape.type === 'polygon'}
          <label class="slider">
            <span>Sides</span>
            <input type="range" min="3" max="12" step="1" aria-label="Sides" value={draft.shape.sides} oninput={(e) => setShapeParam('sides', Number(e.currentTarget.value))} onchange={commit} />
            <output>{Math.min(12, Math.max(3, draft.shape.sides))}</output>
          </label>
        {:else if draft.shape.type === 'sine'}
          <label class="slider">
            <span>Waves</span>
            <input type="range" min="1" max="8" step="1" aria-label="Waves" value={draft.shape.periods} oninput={(e) => setShapeParam('periods', Number(e.currentTarget.value))} onchange={commit} />
            <output>{Math.min(8, Math.max(1, draft.shape.periods))}</output>
          </label>
        {:else if draft.shape.type === 'fan'}
          <p class="fx-hint">The heads of the group open from the center to opposite sides, the outer ones as far as the width, and close again. With <b>Together</b> they open at the same time.</p>
          <label class="slider" title="Part of the cycle spent still, open and closed">
            <span>Hold open and closed</span>
            <input type="range" min="0" max="40" step="1" aria-label="Hold open and closed" value={draft.shape.hold} oninput={(e) => setShapeParam('hold', Number(e.currentTarget.value))} onchange={commit} />
            <output>{Math.min(40, draft.shape.hold)}%</output>
          </label>
        {:else if draft.shape.type === 'lissajous'}
          <label class="slider">
            <span>X waves</span>
            <input type="range" min="1" max="12" step="1" aria-label="X waves" value={draft.shape.x_freq} oninput={(e) => setShapeParam('x_freq', Number(e.currentTarget.value))} onchange={commit} />
            <output>{Math.min(12, Math.max(1, draft.shape.x_freq))}</output>
          </label>
          <label class="slider">
            <span>Y waves</span>
            <input type="range" min="1" max="12" step="1" aria-label="Y waves" value={draft.shape.y_freq} oninput={(e) => setShapeParam('y_freq', Number(e.currentTarget.value))} onchange={commit} />
            <output>{Math.min(12, Math.max(1, draft.shape.y_freq))}</output>
          </label>
        {/if}
      </section>

      <section class="fx-card">
        <h3>Position</h3>
        <p class="fx-hint">Drag the center and the edges on the drawing, or set them here.</p>
        {#each sliders as s (s.key)}
          <label class="slider">
            <span>{s.label}</span>
            <input type="range" min={s.min} max={s.max} step="0.5" bind:value={draft[s.key]} onchange={commit} />
            <input type="number" min={s.min} max={s.max} step="0.5" bind:value={draft[s.key]} onchange={commit} class="num" />°
          </label>
        {/each}
      </section>

      <section class="fx-card">
        <h3>Heads</h3>
        <div class="fx-field">
          <span>Phase between the heads</span>
          <div class="fx-seg" role="group" aria-label="Phase presets">
            {#each PHASES as p (p.value)}
              <button class:active={draft.phase_spread === p.value} aria-pressed={draft.phase_spread === p.value} title={p.title} onclick={() => setPhase(p.value)}>{p.label}</button>
            {/each}
          </div>
        </div>
        <label class="slider">
          <span>Phase spread</span>
          <input type="range" min="0" max="360" step="0.5" bind:value={draft.phase_spread} onchange={commit} />
          <input type="number" min="0" max="360" step="0.5" bind:value={draft.phase_spread} onchange={commit} class="num" />°
        </label>
        {#if draft.shape.type === 'fan'}
          <p class="fx-hint">Together: the heads open at the same time. Wave: they open one after the other.</p>
        {:else}
        <label class="fx-field">
          <span>Mirrored</span>
          <select value={draft.mirror} aria-label="Mirrored heads" title="Heads that draw the shape mirrored left to right" onchange={(e) => { draft!.mirror = e.currentTarget.value as MovementMirror; commit(); }}>
            {#each MIRRORS as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
        {/if}
      </section>

      <FxTiming value={draft} onchange={(patch) => { Object.assign(draft!, patch); commit(); }} />
    </div>
  </div>
{/if}

<style>
  .editor { display: grid; grid-template-columns: minmax(280px, 1fr) minmax(360px, 1.1fr); gap: 20px; align-items: start; }
  @media (max-width: 1100px) { .editor { grid-template-columns: 1fr; } }
  .slider { display: grid; grid-template-columns: 110px minmax(0, 1fr) 72px auto; align-items: center; gap: 8px; }
  .check { display: flex; align-items: center; gap: 8px; }
  .num { width: 72px; }
  .notice { margin: 8px 0 0; font-size: 13px; }
  .notice.warn { color: var(--warn); }
  .small { font-size: 12px; margin: 8px 0 0; }
  .heads { display: flex; flex-wrap: wrap; gap: 4px 12px; list-style: none; padding: 0; margin: 8px 0 0; font-size: 12px; }
  .heads b { display: inline-grid; place-items: center; min-width: 18px; height: 18px; border-radius: 9px; background: var(--text); color: var(--bg, #000); font-size: 11px; margin-right: 2px; }
  .heads li:first-child b { background: var(--ok); }
  .heads li.still { opacity: 0.6; }
</style>
