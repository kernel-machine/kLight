<script lang="ts">
  import { untrack } from 'svelte';
  import { app, run } from '../lib/app.svelte';
  import { groupLayout } from '../lib/core';
  import { colorToHex, formatTime, hexToColor, parseTime } from '../lib/show';
  import type { Color, Function as Fn, MatrixAlgorithm, MatrixEffect } from '../lib/protocol';
  import MatrixPreview from './MatrixPreview.svelte';

  let { f }: { f: Fn } = $props();
  const show = $derived(app.s.show!);
  const source = $derived(f.kind.type === 'matrix_effect' ? f.kind : null);
  let draft = $state<MatrixEffect | null>(null);
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
    const incoming = $state.snapshot(rest) as MatrixEffect;
    untrack(() => {
      const local = draft ? JSON.stringify($state.snapshot(draft)) : null;
      if (id !== syncedId || local === null || local === synced) draft = incoming;
      synced = JSON.stringify(incoming);
      syncedId = id;
    });
  });
  const group = $derived(show.groups.find((g) => g.id === draft?.group));
  const layout = $derived(groupLayout(show, group));
  const labels = $derived(
    (group?.members ?? []).map((m) => `${show.fixtures.find((f) => f.id === m.fixture)?.name ?? m.fixture}${m.head ? ` #${m.head + 1}` : ''}`),
  );

  function commit() {
    if (!draft) return;
    synced = JSON.stringify($state.snapshot(draft));
    run({ type: 'set_function', function: { ...f, kind: { type: 'matrix_effect', ...draft } } });
  }

  const red = hexToColor('#ff0000');
  const blue = hexToColor('#0000ff');
  const algorithms: Record<MatrixAlgorithm['type'], () => MatrixAlgorithm> = {
    solid: () => ({ type: 'solid', color: red }),
    chase: () => ({ type: 'chase', color: red, background: hexToColor('#000000') }),
    wave: () => ({ type: 'wave', color: blue, wavelength: 4 }),
    gradient: () => ({ type: 'gradient', from: red, to: blue }),
    text: () => ({ type: 'text', text: 'KLIGHT', color: red }),
  };

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

{#if draft}
  <div class="editor">
    <MatrixPreview efx={draft} {layout} {labels} bpm={app.s.runtime?.state.tempo.bpm ?? 120} />
    <div class="col">
      <label class="row">Group
        <select value={draft.group} onchange={(e) => { draft!.group = Number(e.currentTarget.value); commit(); }}>
          {#each show.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
        </select>
      </label>
      <div class="row">
        {#each Object.keys(algorithms) as k (k)}
          <button class:active={draft.algorithm.type === k} onclick={() => { draft!.algorithm = algorithms[k as MatrixAlgorithm['type']](); commit(); }}>{k}</button>
        {/each}
      </div>
      {#each colorFields as [key, c] (key)}
        <label class="row">{key} <input type="color" value={colorToHex(c)} oninput={(e) => setField(key, hexToColor(e.currentTarget.value))} onchange={commit} /></label>
      {/each}
      {#if draft.algorithm.type === 'wave'}
        {@const a = draft.algorithm}
        <label class="row">Wavelength <input type="range" min="1" max="32" step="0.5" value={a.wavelength} oninput={(e) => setField('wavelength', Number(e.currentTarget.value))} onchange={commit} /> {a.wavelength} cells</label>
      {/if}
      {#if draft.algorithm.type === 'text'}
        {@const a = draft.algorithm}
        <label class="row">Text <input value={a.text} oninput={(e) => setField('text', e.currentTarget.value)} onchange={commit} /></label>
      {/if}
      <div class="row">
        <button onclick={() => {
          const text = window.prompt('Cycle duration (e.g. 2000, 4s, 2 beats)', formatTime(draft!.period));
          const t = text === null ? null : parseTime(text);
          if (t) { draft!.period = t; commit(); }
        }}>Cycle: {formatTime(draft.period)}</button>
        <button onclick={() => { draft!.direction = draft!.direction === 'Forward' ? 'Backward' : 'Forward'; commit(); }}>Direction: {draft.direction}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .editor { display: grid; grid-template-columns: minmax(260px, 1.2fr) 1fr; gap: 12px; align-items: start; }
  @media (max-width: 1100px) { .editor { grid-template-columns: 1fr; } }
  button { text-transform: capitalize; }
</style>
