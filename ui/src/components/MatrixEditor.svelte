<script lang="ts">
  // Matrix effect editor, laid out as the other FX editors: a header (name,
  // Start/Stop to try it on the lights, rare actions in a menu), the animated
  // grid with its heads numbered in group order, then the controls. Edits and
  // renames go through one queue, each applied to the latest function; the
  // draft follows the engine without undoing edits still on their way.
  import { untrack } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { colorLevel, groupLayout } from '../lib/core';
  import { fxDraft } from '../lib/fxDraft.svelte';
  import { carryColors } from '../lib/matrixColors';
  import { colorToHex, hexToColor } from '../lib/show';
  import FxHeader from './FxHeader.svelte';
  import FxTiming from './FxTiming.svelte';
  import type { Color, Function as Fn, MatrixAlgorithm, MatrixAxis, MatrixControls, MatrixEffect } from '../lib/protocol';
  import MatrixPreview from './MatrixPreview.svelte';

  let { f, onduplicate, ondelete }: { f: Fn; onduplicate: () => void; ondelete: () => void } = $props();
  const show = $derived(app.s.show!);
  const source = $derived(f.kind.type === 'matrix_effect' ? f.kind : null);
  let draft = $state<MatrixEffect | null>(null);

  // Follow the engine's version without undoing edits still on their way.
  const fx = fxDraft<MatrixEffect>('matrix_effect', () => f, () => draft, (d) => (draft = d));
  const { commit, rename } = fx;
  $effect(() => {
    const incoming = source ? fx.strip(source) : null;
    void f.id;
    untrack(() => fx.follow(incoming));
  });
  const group = $derived(show.groups.find((g) => g.id === draft?.group));
  const layout = $derived(groupLayout(show, group));
  const labels = $derived(
    (group?.members ?? []).map((m) => `${show.fixtures.find((f) => f.id === m.fixture)?.name ?? m.fixture}${m.head ? ` #${m.head + 1}` : ''}`),
  );

  const red = hexToColor('#ff0000');
  const blue = hexToColor('#0000ff');
  const black = hexToColor('#000000');
  const ALGORITHMS: Array<{ type: MatrixAlgorithm['type']; label: string; title: string; make: () => MatrixAlgorithm }> = [
    { type: 'solid', label: 'Solid', title: 'One color on every cell', make: () => ({ type: 'solid', color: red }) },
    { type: 'chase', label: 'Chase', title: 'A line of color running across the grid', make: () => ({ type: 'chase', color: red, background: black, width: 1 }) },
    { type: 'wave', label: 'Wave', title: 'A wave of brightness', make: () => ({ type: 'wave', color: blue, wavelength: 4 }) },
    { type: 'gradient', label: 'Gradient', title: 'Two colors blending, scrolling', make: () => ({ type: 'gradient', from: red, to: blue }) },
    { type: 'fill', label: 'Fill', title: 'The grid fills with the color, one line at a time', make: () => ({ type: 'fill', color: red, background: black }) },
    { type: 'rainbow', label: 'Rainbow', title: 'Every hue, scrolling along the grid', make: () => ({ type: 'rainbow', spread: 1 }) },
    { type: 'twinkle', label: 'Twinkle', title: 'Cells lighting up at random', make: () => ({ type: 'twinkle', color: hexToColor('#ffffff'), background: black, density: 0.3 }) },
    { type: 'text', label: 'Text', title: 'Text scrolling across the grid', make: () => ({ type: 'text', text: 'KLIGHT', color: red }) },
  ];
  /** Algorithms that move along an axis. */
  const ALONG = new Set<MatrixAlgorithm['type']>(['chase', 'wave', 'gradient', 'fill', 'rainbow']);
  const COLOR_NAMES: Record<string, string> = { color: 'Color', background: 'Background', from: 'From', to: 'To' };
  const LEVEL_NAMES: Record<string, string> = { color: 'Level', background: 'Background level', from: 'From', to: 'To' };
  const CONTROLS: Array<{ value: MatrixControls; label: string; title: string }> = [
    { value: 'color', label: 'Color', title: 'The pattern sets the color of the lights' },
    { value: 'dimmer', label: 'Dimmer', title: 'The pattern sets only the intensity; the colors come from the scenes' },
  ];
  /** On the dimmer, a gray of the level: what the engine reads back is the level. */
  const gray = (l: number): Color => ({ r: l, g: l, b: l });
  const AXES: Array<[MatrixAxis, string, string]> = [
    ['horizontal', 'Columns', 'Moves across the columns, left to right'],
    ['vertical', 'Rows', 'Moves across the rows, top to bottom'],
    ['radial', 'From the center', 'Moves out from the center of the grid, in rings'],
  ];

  /** Color fields of the current algorithm, editable with color pickers. */
  const colorFields = $derived.by(() => {
    const a = draft?.algorithm;
    if (!a) return [] as Array<[string, Color]>;
    return Object.entries(a).filter(([, v]) => typeof v === 'object' && v !== null && 'r' in v) as Array<[string, Color]>;
  });

  function setField(key: string, value: unknown) {
    if (!draft) return;
    draft.algorithm = { ...draft.algorithm, [key]: value } as MatrixAlgorithm;
  }
</script>

<FxHeader {f} badge="Matrix" nameLabel="Matrix name" deleteLabel="Delete matrix" onrename={rename} {onduplicate} {ondelete} />

{#if draft}
  <div class="editor">
    <div class="fx-preview">
      <MatrixPreview efx={draft} {layout} {labels} bpm={app.s.runtime?.state.tempo.bpm ?? 120} />
    </div>
    <div class="fx-sections">
      <section class="fx-card">
        <h3>Lights</h3>
        <label class="fx-field">
          <span>Group</span>
          <select value={draft.group} aria-label="Matrix group" onchange={(e) => { draft!.group = Number(e.currentTarget.value); commit(); }}>
            {#each show.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
          </select>
        </label>
        <div class="fx-field">
          <span>The pattern controls</span>
          <div class="fx-seg" role="group" aria-label="Controls">
            {#each CONTROLS as c (c.value)}
              <button class:active={draft.controls === c.value} aria-pressed={draft.controls === c.value} disabled={c.value === 'dimmer' && draft.algorithm.type === 'rainbow'} title={c.value === 'dimmer' && draft.algorithm.type === 'rainbow' ? 'A rainbow has no brightness to vary: it needs the Color controls' : c.title} onclick={() => { draft!.controls = c.value; commit(); }}>{c.label}</button>
            {/each}
          </div>
          {#if draft.controls === 'dimmer'}
            <p class="fx-hint" data-dimmer-hint>
              Only the intensity: give these lights their colors with a scene. Intensity adds up (highest wins), so a
              scene that also sets it keeps the lights at least that bright.
            </p>
          {:else}
            <p class="fx-hint">The color of the lights, and their intensity along with it.</p>
          {/if}
        </div>
      </section>

      <section class="fx-card">
        <h3>Pattern</h3>
        <div class="fx-chips" role="group" aria-label="Algorithm">
          {#each ALGORITHMS as a (a.type)}
            <button class:active={draft.algorithm.type === a.type} aria-pressed={draft.algorithm.type === a.type} disabled={a.type === 'rainbow' && draft.controls === 'dimmer'} title={a.type === 'rainbow' && draft.controls === 'dimmer' ? 'A rainbow is all colors: it needs the Color controls' : a.title} onclick={() => { draft!.algorithm = carryColors(draft!.algorithm, a.make()); commit(); }}>{a.label}</button>
          {/each}
        </div>
        {#if draft.algorithm.type === 'wave'}
          {@const a = draft.algorithm}
          <label class="fx-field level">
            <span>Wavelength</span>
            <input type="range" min="1" max="32" step="0.5" aria-label="Wavelength" value={a.wavelength} oninput={(e) => setField('wavelength', Number(e.currentTarget.value))} onchange={commit} />
            <span class="pct">{a.wavelength} cells</span>
          </label>
        {/if}
        {#if draft.algorithm.type === 'rainbow'}
          {@const a = draft.algorithm}
          <label class="fx-field level">
            <span>Rainbows</span>
            <input type="range" min="0.25" max="4" step="0.25" aria-label="Rainbows" value={a.spread} oninput={(e) => setField('spread', Number(e.currentTarget.value))} onchange={commit} />
            <span class="pct">{a.spread}</span>
          </label>
        {/if}
        {#if draft.algorithm.type === 'twinkle'}
          {@const a = draft.algorithm}
          <label class="fx-field level">
            <span>Density</span>
            <input type="range" min="0" max="1" step="0.05" aria-label="Density" value={a.density} oninput={(e) => setField('density', Number(e.currentTarget.value))} onchange={commit} />
            <span class="pct">{Math.round(a.density * 100)}%</span>
          </label>
        {/if}
        {#if draft.algorithm.type === 'text'}
          {@const a = draft.algorithm}
          <label class="fx-field"><span>Text</span> <input value={a.text} oninput={(e) => setField('text', e.currentTarget.value)} onchange={commit} /></label>
        {/if}
        {#if draft.algorithm.type === 'chase'}
          {@const a = draft.algorithm}
          <label class="fx-field level">
            <span>Width</span>
            <input type="range" min="1" max="16" step="1" aria-label="Chase width" value={a.width} oninput={(e) => setField('width', Number(e.currentTarget.value))} onchange={commit} />
            <span class="pct">{Math.min(16, Math.max(1, a.width))} {a.width === 1 ? 'cell' : 'cells'}</span>
          </label>
        {/if}
        {#if ALONG.has(draft.algorithm.type)}
          <div class="fx-field">
            <span>Moves</span>
            <div class="fx-seg" role="group" aria-label="Axis">
              {#each AXES as [v, label, title] (v)}
                <button class:active={draft.axis === v} aria-pressed={draft.axis === v} {title} onclick={() => { draft!.axis = v; commit(); }}>{label}</button>
              {/each}
            </div>
          </div>
        {/if}
      </section>

      {#if colorFields.length > 0}
      <section class="fx-card">
        <h3>{draft.controls === 'dimmer' ? 'Levels' : 'Colors'}</h3>
        {#if draft.controls === 'dimmer'}
          {#each colorFields as [key, c] (key)}
            {@const level = colorLevel(c)}
            <label class="fx-field level">
              <span>{LEVEL_NAMES[key] ?? key}</span>
              <input type="range" min="0" max="1" step="0.01" aria-label={LEVEL_NAMES[key] ?? key} value={level} oninput={(e) => setField(key, gray(Number(e.currentTarget.value)))} onchange={commit} />
              <span class="pct">{Math.round(level * 100)}%</span>
            </label>
          {/each}
        {:else if colorFields.length > 0}
          <div class="colors">
            {#each colorFields as [key, c] (key)}
              <label class="swatch"><span>{COLOR_NAMES[key] ?? key}</span> <input type="color" aria-label={COLOR_NAMES[key] ?? key} value={colorToHex(c)} oninput={(e) => setField(key, hexToColor(e.currentTarget.value))} onchange={commit} /></label>
            {/each}
          </div>
        {/if}
      </section>
      {/if}

      <FxTiming value={draft} onchange={(patch) => { Object.assign(draft!, patch); commit(); }} />
    </div>
  </div>
{/if}

<style>
  .editor { display: grid; grid-template-columns: minmax(260px, 1fr) minmax(340px, 1.1fr); gap: 20px; align-items: start; }
  @media (max-width: 1100px) { .editor { grid-template-columns: 1fr; } }
  .colors { display: flex; flex-wrap: wrap; gap: 20px; }
  .swatch { display: flex; align-items: center; gap: 8px; }
  .swatch input { width: 44px; height: 32px; padding: 2px; }
  .level { display: grid; grid-template-columns: 120px minmax(0, 1fr) 70px; gap: 10px; align-items: center; }
  .level > span:first-child { font-size: 13px; color: var(--text); }
  .pct { text-align: right; font-variant-numeric: tabular-nums; }
</style>
