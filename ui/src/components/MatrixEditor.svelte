<script lang="ts">
  // Matrix effect editor, laid out as the other FX editors: a header (name,
  // Start/Stop to try it on the lights, rare actions in a menu), the animated
  // grid with its heads numbered in group order, then the controls. Edits and
  // renames go through one queue, each applied to the latest function; the
  // draft follows the engine without undoing edits still on their way.
  import { untrack } from 'svelte';
  import { app, notify, run } from '../lib/app.svelte';
  import { groupLayout } from '../lib/core';
  import { draftSync } from '../lib/draftSync';
  import { editQueue } from '../lib/queue';
  import { colorToHex, hexToColor } from '../lib/show';
  import TimeSelect from './TimeSelect.svelte';
  import type { Color, Function as Fn, MatrixAlgorithm, MatrixAxis, MatrixEffect, RunMode } from '../lib/protocol';
  import MatrixPreview from './MatrixPreview.svelte';

  let { f, onduplicate, ondelete }: { f: Fn; onduplicate: () => void; ondelete: () => void } = $props();
  const show = $derived(app.s.show!);
  const source = $derived(f.kind.type === 'matrix_effect' ? f.kind : null);
  const running = $derived((app.s.runtime?.state.active ?? []).some((a) => a.id === f.id));
  let draft = $state<MatrixEffect | null>(null);

  // Follow the engine's version without undoing edits still on their way.
  const sync = draftSync<MatrixEffect>();
  const strip = (kind: Fn['kind']): MatrixEffect | null => {
    if (kind.type !== 'matrix_effect') return null;
    const { type: _type, ...rest } = kind;
    return $state.snapshot(rest) as MatrixEffect;
  };
  $effect(() => {
    const incoming = source ? strip(source) : null;
    const id = f.id;
    if (!incoming) return;
    untrack(() => {
      const shown = sync.incoming(id, incoming, draft ? ($state.snapshot(draft) as MatrixEffect) : null);
      if (shown) draft = shown;
    });
  });
  const group = $derived(show.groups.find((g) => g.id === draft?.group));
  const layout = $derived(groupLayout(show, group));
  const labels = $derived(
    (group?.members ?? []).map((m) => `${show.fixtures.find((f) => f.id === m.fixture)?.name ?? m.fixture}${m.head ? ` #${m.head + 1}` : ''}`),
  );

  const enqueue = editQueue((e) => notify(String(e), 'error'));
  const latest = (id: number) => app.s.show?.functions.find((x) => x.id === id);

  function commit() {
    if (!draft) return;
    const kind = $state.snapshot(draft) as MatrixEffect;
    sync.sent(kind);
    const id = f.id;
    enqueue(async () => {
      const current = latest(id);
      if (!current || current.kind.type !== 'matrix_effect') return;
      const ok = await run({ type: 'set_function', function: { ...current, kind: { type: 'matrix_effect', ...kind } } });
      const now = latest(id);
      const engine = now ? strip(now.kind) : null;
      if (!engine || f.id !== id) return;
      const local = draft ? ($state.snapshot(draft) as MatrixEffect) : null;
      if (ok) {
        const shown = sync.settled(kind, engine, local);
        if (shown) draft = shown;
        return;
      }
      // Refused: back to what the engine has.
      sync.refused();
      draft = sync.incoming(id, engine, null);
    });
  }

  /** Renamed on the latest version: an edit not confirmed yet is kept. */
  function rename(name: string) {
    const id = f.id;
    enqueue(() => {
      const current = latest(id);
      if (!current || current.name === name) return;
      return run({ type: 'set_function', function: { ...current, name } });
    });
  }

  const red = hexToColor('#ff0000');
  const blue = hexToColor('#0000ff');
  const black = hexToColor('#000000');
  const ALGORITHMS: Array<{ type: MatrixAlgorithm['type']; label: string; title: string; make: () => MatrixAlgorithm }> = [
    { type: 'solid', label: 'Solid', title: 'One color on every cell', make: () => ({ type: 'solid', color: red }) },
    { type: 'chase', label: 'Chase', title: 'A line of color running across the grid', make: () => ({ type: 'chase', color: red, background: black }) },
    { type: 'wave', label: 'Wave', title: 'A wave of brightness', make: () => ({ type: 'wave', color: blue, wavelength: 4 }) },
    { type: 'gradient', label: 'Gradient', title: 'Two colors blending, scrolling', make: () => ({ type: 'gradient', from: red, to: blue }) },
    { type: 'fill', label: 'Fill', title: 'The grid fills with the color, one line at a time', make: () => ({ type: 'fill', color: red, background: black }) },
    { type: 'twinkle', label: 'Twinkle', title: 'Cells lighting up at random', make: () => ({ type: 'twinkle', color: hexToColor('#ffffff'), background: black, density: 0.3 }) },
    { type: 'text', label: 'Text', title: 'Text scrolling across the grid', make: () => ({ type: 'text', text: 'KLIGHT', color: red }) },
  ];
  /** Algorithms that move along an axis. */
  const ALONG = new Set<MatrixAlgorithm['type']>(['chase', 'wave', 'gradient', 'fill']);
  const COLOR_NAMES: Record<string, string> = { color: 'Color', background: 'Background', from: 'From', to: 'To' };
  const RUN_MODES: Array<[RunMode, string]> = [['Loop', 'Loop'], ['SingleShot', 'Single shot'], ['PingPong', 'Ping-pong']];
  const AXES: Array<[MatrixAxis, string]> = [['horizontal', 'Across the columns'], ['vertical', 'Across the rows']];

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

  const toggleRun = () => run(running ? { type: 'stop_function', id: f.id } : { type: 'start_function', id: f.id });

  // ---- the header's menu of rarely used actions.
  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  $effect(() => {
    void f.id;
    menuOpen = false;
  });
  const act = (fn: () => void) => () => {
    menuOpen = false;
    fn();
  };
  function outside(e: MouseEvent) {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }
</script>

<svelte:window onclick={outside} />

<div class="head">
  <input class="name" value={f.name} aria-label="Matrix name" onchange={(e) => rename(e.currentTarget.value)} />
  <span class="badge">Matrix</span>
  <span class="spacer"></span>
  {#if running}
    <button class="primary run" onclick={toggleRun}>Stop</button>
  {:else}
    <button class="run" onclick={toggleRun} title="Plays it on the lights">Start</button>
  {/if}
  <div class="menu-wrap" bind:this={menuWrap}>
    <button class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>⋯</button>
    {#if menuOpen}
      <div class="menu" role="menu" tabindex="-1" onkeydown={(e) => { if (e.key === 'Escape') menuOpen = false; }}>
        <button role="menuitem" onclick={act(onduplicate)}>Duplicate</button>
        <hr />
        <button role="menuitem" class="danger-text" onclick={act(ondelete)}>Delete matrix</button>
      </div>
    {/if}
  </div>
</div>

{#if draft}
  <div class="editor">
    <MatrixPreview efx={draft} {layout} {labels} bpm={app.s.runtime?.state.tempo.bpm ?? 120} />
    <div class="col">
      <label class="row">Group
        <select value={draft.group} aria-label="Matrix group" onchange={(e) => { draft!.group = Number(e.currentTarget.value); commit(); }}>
          {#each show.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
        </select>
      </label>
      <div class="algos" role="group" aria-label="Algorithm">
        {#each ALGORITHMS as a (a.type)}
          <button class:active={draft.algorithm.type === a.type} title={a.title} onclick={() => { draft!.algorithm = a.make(); commit(); }}>{a.label}</button>
        {/each}
      </div>
      <div class="row colors">
        {#each colorFields as [key, c] (key)}
          <label class="row">{COLOR_NAMES[key] ?? key} <input type="color" aria-label={COLOR_NAMES[key] ?? key} value={colorToHex(c)} oninput={(e) => setField(key, hexToColor(e.currentTarget.value))} onchange={commit} /></label>
        {/each}
      </div>
      {#if draft.algorithm.type === 'wave'}
        {@const a = draft.algorithm}
        <label class="row">Wavelength <input type="range" min="1" max="32" step="0.5" value={a.wavelength} oninput={(e) => setField('wavelength', Number(e.currentTarget.value))} onchange={commit} /> {a.wavelength} cells</label>
      {/if}
      {#if draft.algorithm.type === 'twinkle'}
        {@const a = draft.algorithm}
        <label class="row">Density <input type="range" min="0" max="1" step="0.05" aria-label="Density" value={a.density} oninput={(e) => setField('density', Number(e.currentTarget.value))} onchange={commit} /> {Math.round(a.density * 100)}% of the cells</label>
      {/if}
      {#if draft.algorithm.type === 'text'}
        {@const a = draft.algorithm}
        <label class="row">Text <input value={a.text} oninput={(e) => setField('text', e.currentTarget.value)} onchange={commit} /></label>
      {/if}
      {#if ALONG.has(draft.algorithm.type)}
        <label class="row">Moves
          <select aria-label="Axis" value={draft.axis} onchange={(e) => { draft!.axis = e.currentTarget.value as MatrixAxis; commit(); }}>
            {#each AXES as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
      {/if}
      <div class="row">
        <TimeSelect label="Cycle" value={draft.period} onchange={(period) => { draft!.period = period; commit(); }} />
        <button onclick={() => { draft!.direction = draft!.direction === 'Forward' ? 'Backward' : 'Forward'; commit(); }}>Direction: {draft.direction}</button>
        <label class="row">Mode
          <select value={draft.run_mode} aria-label="Run mode" onchange={(e) => { draft!.run_mode = e.currentTarget.value as RunMode; commit(); }}>
            {#each RUN_MODES as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
      </div>
      <div class="row">
        <TimeSelect label="Fade in" zero value={draft.fade_in} onchange={(fade_in) => { draft!.fade_in = fade_in; commit(); }} />
        <TimeSelect label="Fade out" zero value={draft.fade_out} onchange={(fade_out) => { draft!.fade_out = fade_out; commit(); }} />
      </div>
    </div>
  </div>
{/if}

<style>
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; }
  .head .name { font-size: 18px; font-weight: 600; min-width: 0; flex: 0 1 280px; }
  .spacer { flex: 1; }
  .run { min-height: 36px; padding: 0 16px; }
  .menu-wrap { position: relative; }
  .more { width: 38px; min-height: 36px; padding: 0; font-size: 18px; line-height: 1; }
  .menu { position: absolute; right: 0; top: calc(100% + 6px); z-index: 20; min-width: 220px; display: flex; flex-direction: column; padding: 6px; background: var(--panel-2); border: 1px solid var(--border); border-radius: var(--radius); box-shadow: 0 10px 28px rgb(0 0 0 / 45%); }
  .menu button { text-align: left; border: none; background: none; padding: 8px 10px; }
  .menu button:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .menu hr { width: 100%; border: none; border-top: 1px solid var(--border); margin: 4px 0; }
  .danger-text { color: var(--danger, #e5484d); }
  .editor { display: grid; grid-template-columns: minmax(260px, 1.2fr) 1fr; gap: 12px; align-items: start; }
  @media (max-width: 1100px) { .editor { grid-template-columns: 1fr; } }
  .algos { display: flex; flex-wrap: wrap; gap: 4px; }
  .colors { flex-wrap: wrap; gap: 12px; }
</style>
