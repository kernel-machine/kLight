<script lang="ts">
  // Functions: scenes, chasers, cue lists, effects, palettes.
  import { app, run } from '../lib/app.svelte';
  import { colorSequence, colorToHex, describeTarget, describeValue, formatTime, hexToColor, maxSequenceColors, nextId, parseTime } from '../lib/show';
  import type { Cue, Function as Fn, FunctionKind, SceneValue, TimeValue } from '../lib/protocol';
  import SceneEditor from '../components/SceneEditor.svelte';
  import MovementEditor from '../components/MovementEditor.svelte';
  import MatrixEditor from '../components/MatrixEditor.svelte';
  import FunctionTree from '../components/FunctionTree.svelte';

  const show = $derived(app.s.show!);
  const active = $derived(new Map((app.s.runtime?.state.active ?? []).map((a) => [a.id, a])));
  const programmer = $derived(app.s.runtime?.state.programmer ?? []);
  let selectedId = $state<number | null>(null);
  const selected = $derived(show.functions.find((f) => f.id === selectedId) ?? null);
  const scenes = $derived(show.functions.filter((f) => f.kind.type === 'scene'));

  function kindLabel(k: FunctionKind['type']) {
    return { scene: 'Scene', chaser: 'Chaser', cue_list: 'Cue list', movement_effect: 'Movement', matrix_effect: 'Matrix' }[k];
  }

  function save(f: Fn) {
    return run({ type: 'set_function', function: f });
  }

  function timeInput(current: TimeValue, apply: (t: TimeValue) => void) {
    const text = window.prompt('Time (e.g. 500, 1.5s, 2 beats)', formatTime(current));
    if (text === null) return;
    const t = parseTime(text);
    if (t) apply(t);
  }

  async function create(type: FunctionKind['type'], folder: number | null = null) {
    const id = nextId(show.functions);
    const firstGroup = show.groups[0]?.id ?? 0;
    const kinds: Record<FunctionKind['type'], FunctionKind> = {
      scene: { type: 'scene', values: [], timing: { delay: { ms: 0 }, fade_in: { ms: 0 }, fade_out: { ms: 0 } }, spread: { type: 'none' }, attribute_timing: [] },
      chaser: { type: 'chaser', steps: [], hold: { beats: 1 }, fade: { ms: 0 }, direction: 'Forward', run_mode: 'Loop' },
      cue_list: { type: 'cue_list', cues: [], move_in_black: true },
      movement_effect: { type: 'movement_effect', group: firstGroup, shape: { type: 'circle' }, center_pan: 0, center_tilt: 0, width: 60, height: 30, period: { beats: 4 }, phase_spread: 0, direction: 'Forward' },
      matrix_effect: { type: 'matrix_effect', group: firstGroup, algorithm: { type: 'chase', color: hexToColor('#ff0000'), background: hexToColor('#000000') }, period: { beats: 2 }, direction: 'Forward' },
    };
    if (await save({ id, name: `${kindLabel(type)} ${id}`, kind: kinds[type], folder })) selectedId = id;
  }

  const programmerValues = (): SceneValue[] => programmer.map((e) => ({ target: e.target, value: { type: 'explicit', value: e.value } }));

  /** Folders of a type with their full path, for the move selector. */
  function folderPaths(category: FunctionKind['type']): Array<[number, string]> {
    const path = (id: number | null | undefined, depth = 0): string => {
      const folder = show.function_folders.find((x) => x.id === id);
      if (!folder || depth > 32) return '';
      const up = path(folder.parent, depth + 1);
      return up ? `${up} / ${folder.name}` : folder.name;
    };
    return show.function_folders
      .filter((x) => x.category === category)
      .map((x) => [x.id, path(x.id)] as [number, string])
      .sort((a, b) => a[1].localeCompare(b[1], undefined, { numeric: true }));
  }

  // Color sequence: one scene per color, for a group. The colors come from the core.
  const maxColors = maxSequenceColors();
  let seqGroup = $state<number | null>(null);
  let seqCount = $state(3);
  let seqBusy = $state(false);
  const seqGroupId = $derived(show.groups.some((g) => g.id === seqGroup) ? seqGroup : (show.groups[0]?.id ?? null));
  const seqN = $derived(Math.min(maxColors, Math.max(1, Math.floor(Number.isFinite(seqCount) ? seqCount : 1))));
  const seqPreview = $derived(colorSequence(seqN));

  async function generateColorScenes() {
    const group = show.groups.find((g) => g.id === seqGroupId);
    if (!group || seqBusy) return;
    seqBusy = true;
    try {
      const folder = nextId(show.function_folders);
      if (!(await run({ type: 'set_function_folder', folder: { id: folder, category: 'scene', name: `${group.name} colors`, parent: null } }))) return;
      const first = nextId(show.functions);
      const target = { type: 'group', id: group.id } as const;
      for (const [i, c] of seqPreview.entries()) {
        const values: SceneValue[] = [
          { target, value: { type: 'explicit', value: { type: 'intensity', level: 1 } } },
          { target, value: { type: 'explicit', value: { type: 'color', color: c.color } } },
        ];
        const kind: FunctionKind = { type: 'scene', values, timing: { delay: { ms: 0 }, fade_in: { ms: 0 }, fade_out: { ms: 0 } }, spread: { type: 'none' }, attribute_timing: [] };
        if (!(await save({ id: first + i, name: c.name, kind, folder }))) return;
      }
    } finally {
      seqBusy = false;
    }
  }

  function updateKind(f: Fn, kind: FunctionKind) {
    save({ ...f, kind });
  }
</script>

<div class="fx">
  <section class="panel list">
    <h2>Functions</h2>
    <FunctionTree {selectedId} onselect={(id) => (selectedId = id)} oncreate={(category, folder) => create(category, folder)} />
    <details class="seq">
      <summary>Color sequence</summary>
    {#if show.groups.length}
      <div class="row">
        <select aria-label="Group" value={seqGroupId} onchange={(e) => (seqGroup = Number(e.currentTarget.value))}>
          {#each show.groups as g (g.id)}<option value={g.id}>{g.name}</option>{/each}
        </select>
        <label class="row">Colors
          <input type="number" min="1" max={maxColors} step="1" style="width: 5em" bind:value={seqCount} />
        </label>
        <button class="primary" disabled={seqBusy} onclick={generateColorScenes}>Create {seqN} scene{seqN === 1 ? '' : 's'}</button>
      </div>
      <div class="row swatches">
        {#each seqPreview as c (c.name)}
          <span class="swatch" title={c.name}><i style="background:{colorToHex(c.color)}"></i>{c.name}</span>
        {/each}
      </div>
      <span class="muted">Up to {maxColors} colors. One scene per color, in a folder, at full intensity.</span>
    {:else}<span class="muted">Create a fixture group first.</span>{/if}
    </details>
    <h3>Palettes</h3>
    {#each show.palettes as p (p.id)}
      <div class="row"><span>{p.name}</span><span class="badge">{p.kind}</span><span class="muted">{p.per_fixture.length} fixtures</span>
        <button class="danger" onclick={() => run({ type: 'remove_palette', id: p.id })}>×</button></div>
    {:else}<span class="muted">Create palettes from the Simple Desk.</span>{/each}
  </section>

  <section class="panel editor">
    {#if !selected}
      <span class="muted">Select a function to edit it.</span>
    {:else}
      {@const f = selected}
      <div class="row">
        <input value={f.name} onchange={(e) => save({ ...f, name: e.currentTarget.value })} />
        <span class="badge">{kindLabel(f.kind.type)}</span>
        <label class="row">Folder
          <select value={f.folder ?? ''} aria-label="Folder" onchange={(e) => save({ ...f, folder: e.currentTarget.value === '' ? null : Number(e.currentTarget.value) })}>
            <option value="">— top level —</option>
            {#each folderPaths(f.kind.type) as [id, path] (id)}<option value={id}>{path}</option>{/each}
          </select>
        </label>
        <button class="danger" onclick={() => { run({ type: 'remove_function', id: f.id }); selectedId = null; }}>Delete</button>
      </div>

      {#if f.kind.type === 'scene'}
        {@const k = f.kind}
        <h3>Timing</h3>
        <div class="row">
          <button onclick={() => timeInput(k.timing.fade_in, (t) => updateKind(f, { ...k, timing: { ...k.timing, fade_in: t } }))}>Fade in: {formatTime(k.timing.fade_in)}</button>
          <button onclick={() => timeInput(k.timing.fade_out, (t) => updateKind(f, { ...k, timing: { ...k.timing, fade_out: t } }))}>Fade out: {formatTime(k.timing.fade_out)}</button>
          <button onclick={() => timeInput(k.timing.delay, (t) => updateKind(f, { ...k, timing: { ...k.timing, delay: t } }))}>Delay: {formatTime(k.timing.delay)}</button>
          <button onclick={() => timeInput(k.spread.type === 'none' ? { ms: 0 } : k.spread.step, (t) => updateKind(f, { ...k, spread: 'ms' in t && t.ms === 0 ? { type: 'none' } : { type: 'linear', step: t } }))}>
            Spread over group: {k.spread.type === 'none' ? 'off' : formatTime(k.spread.step)}
          </button>
        </div>
        <SceneEditor {f} scene={k} />
      {:else if f.kind.type === 'chaser'}
        {@const k = f.kind}
        <div class="row">
          <button onclick={() => timeInput(k.hold, (t) => updateKind(f, { ...k, hold: t }))}>Step: {formatTime(k.hold)}</button>
          <button onclick={() => timeInput(k.fade, (t) => updateKind(f, { ...k, fade: t }))}>Fade: {formatTime(k.fade)}</button>
          <select value={k.run_mode} onchange={(e) => updateKind(f, { ...k, run_mode: e.currentTarget.value as typeof k.run_mode })}>
            <option>Loop</option><option>SingleShot</option><option>PingPong</option>
          </select>
          <select value={k.direction} onchange={(e) => updateKind(f, { ...k, direction: e.currentTarget.value as typeof k.direction })}>
            <option>Forward</option><option>Backward</option>
          </select>
        </div>
        <h3>Steps</h3>
        {#each k.steps as s, i (i)}
          <div class="row" class:current={active.get(f.id)?.position === i}>
            <span>{i + 1}. {show.functions.find((x) => x.id === s.scene)?.name ?? `#${s.scene}`}</span>
            <button onclick={() => updateKind(f, { ...k, steps: k.steps.filter((_, j) => j !== i) })}>×</button>
          </div>
        {/each}
        <div class="row">
          {#each scenes as s (s.id)}<button onclick={() => updateKind(f, { ...k, steps: [...k.steps, { scene: s.id, hold: null, fade: null }] })}>+ {s.name}</button>{/each}
        </div>
      {:else if f.kind.type === 'cue_list'}
        {@const k = f.kind}
        {@const pos = active.get(f.id)?.position}
        <div class="row">
          <button class="primary big" onclick={() => run({ type: 'cue_go', id: f.id })}>GO</button>
          <button onclick={() => run({ type: 'cue_back', id: f.id })}>Back</button>
          <button onclick={() => run({ type: 'cue_pause', id: f.id })}>{active.get(f.id)?.paused ? 'Resume' : 'Pause'}</button>
          <label class="row"><input type="checkbox" checked={k.move_in_black} onchange={(e) => updateKind(f, { ...k, move_in_black: e.currentTarget.checked })} /> Move in black</label>
        </div>
        <table>
          <thead><tr><th>#</th><th>Name</th><th>Fade</th><th>Values</th><th></th></tr></thead>
          <tbody>
            {#each k.cues as c, i (i)}
              <tr class:current={pos === i}>
                <td>{c.number}</td><td>{c.name}</td>
                <td><button onclick={() => timeInput(c.timing.fade_in, (t) => updateKind(f, { ...k, cues: k.cues.map((x, j) => (j === i ? { ...x, timing: { ...x.timing, fade_in: t, fade_out: t } } : x)) }))}>{formatTime(c.timing.fade_in)}</button></td>
                <td>{c.values.length}</td>
                <td><button onclick={() => run({ type: 'cue_go_to', id: f.id, index: i })}>Go to</button>
                  <button onclick={() => updateKind(f, { ...k, cues: k.cues.filter((_, j) => j !== i) })}>×</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
        <button disabled={!programmer.length} onclick={() => {
          const cue: Cue = { number: k.cues.length + 1, name: `Cue ${k.cues.length + 1}`, values: programmerValues(), timing: { delay: { ms: 0 }, fade_in: { ms: 2000 }, fade_out: { ms: 2000 } }, follow: { type: 'manual' }, move_in_black: null };
          updateKind(f, { ...k, cues: [...k.cues, cue] });
        }}>Record cue from programmer</button>
      {:else if f.kind.type === 'movement_effect'}
        <MovementEditor {f} />
      {:else if f.kind.type === 'matrix_effect'}
        <MatrixEditor {f} />
      {/if}
    {/if}
  </section>
</div>

<style>
  .fx { display: grid; grid-template-columns: minmax(280px, 1fr) minmax(0, 2fr); gap: 12px; }
  @media (max-width: 900px) { .fx { grid-template-columns: minmax(0, 1fr); } }
  /* A wide row of faders scrolls inside the editor instead of widening it. */
  .editor { min-width: 0; }
  .seq { margin-top: 12px; }
  .seq summary { cursor: pointer; font-weight: 600; margin-bottom: 8px; }
  tr { cursor: pointer; }
  .swatches { flex-wrap: wrap; gap: 6px; }
  .swatch { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; }
  .swatch i { width: 12px; height: 12px; border-radius: 3px; border: 1px solid var(--border); }
  .current { background: color-mix(in srgb, var(--accent) 20%, transparent); }
  .big { font-size: 20px; padding: 10px 28px; }
</style>
