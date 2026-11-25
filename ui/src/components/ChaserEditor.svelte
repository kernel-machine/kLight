<script lang="ts">
  // Chaser contents: step times, run mode and the ordered list of scenes.
  import { app, run } from '../lib/app.svelte';
  import { colorToHex, formatTime, functionColor } from '../lib/show';
  import FunctionPickerDialog from './FunctionPickerDialog.svelte';
  import type { Chaser, ChaserStep, Function as Fn, TimeValue } from '../lib/protocol';

  let {
    f,
    chaser,
    timeInput,
    picking = $bindable(),
  }: {
    f: Fn;
    chaser: Chaser;
    timeInput: (current: TimeValue, apply: (t: TimeValue) => void) => void;
    /** Chaser whose "add scenes" dialog is open. */
    picking: number | null;
  } = $props();

  const show = $derived(app.s.show!);
  const position = $derived(app.s.runtime?.state.active.find((a) => a.id === f.id)?.position);
  const hasScenes = $derived(show.functions.some((x) => x.kind.type === 'scene'));

  // Edits are queued and each one is applied to the latest version of the
  // chaser: quick successive clicks must not overwrite each other.
  let queue: Promise<unknown> = Promise.resolve();
  function update(change: (chaser: Chaser) => Partial<Chaser>) {
    const id = f.id;
    queue = queue.then(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.kind.type !== 'chaser') return;
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, ...change(current.kind) } } });
    });
  }

  const updateSteps = (change: (steps: ChaserStep[]) => ChaserStep[]) => update((c) => ({ steps: change(c.steps) }));

  function addScenes(ids: number[]) {
    picking = null;
    updateSteps((steps) => [...steps, ...ids.map((scene) => ({ scene, hold: null, fade: null }))]);
  }

  /** Moves the step at `from` so that it ends up at `to`. */
  function move(from: number, to: number) {
    updateSteps((steps) => {
      if (from === to || from < 0 || to < 0 || from >= steps.length || to >= steps.length) return steps;
      const next = [...steps];
      next.splice(to, 0, ...next.splice(from, 1));
      return next;
    });
  }

  // ---- drag & drop
  let dragged = $state<number | null>(null);
  let over = $state<number | null>(null);

  function drop(to: number) {
    if (dragged !== null) move(dragged, to);
    dragged = null;
    over = null;
  }

  const sceneOf = (step: ChaserStep) => show.functions.find((x) => x.id === step.scene);
</script>

<div class="row">
  <button onclick={() => timeInput(chaser.hold, (hold) => update(() => ({ hold })))}>Step: {formatTime(chaser.hold)}</button>
  <button onclick={() => timeInput(chaser.fade, (fade) => update(() => ({ fade })))}>Fade: {formatTime(chaser.fade)}</button>
  <select value={chaser.run_mode} aria-label="Run mode" onchange={(e) => { const run_mode = e.currentTarget.value as Chaser['run_mode']; update(() => ({ run_mode })); }}>
    <option>Loop</option><option>SingleShot</option><option>PingPong</option>
  </select>
  <select value={chaser.direction} aria-label="Direction" onchange={(e) => { const direction = e.currentTarget.value as Chaser['direction']; update(() => ({ direction })); }}>
    <option>Forward</option><option>Backward</option>
  </select>
</div>

<h3>Steps ({chaser.steps.length})</h3>
<ol class="steps" aria-label="Chaser steps">
  {#each chaser.steps as step, i (i)}
    {@const scene = sceneOf(step)}
    {@const color = scene ? functionColor(show, scene) : null}
    <li
      class:current={position === i}
      class:over={over === i && dragged !== i}
      draggable="true"
      ondragstart={(e) => { dragged = i; e.dataTransfer?.setData('text/plain', `s${i}`); }}
      ondragend={() => { dragged = null; over = null; }}
      ondragover={(e) => { if (dragged !== null) { e.preventDefault(); over = i; } }}
      ondragleave={() => { if (over === i) over = null; }}
      ondrop={(e) => { e.preventDefault(); drop(i); }}
    >
      <span class="grip muted" aria-hidden="true">⠿</span>
      <span class="number">{i + 1}.</span>
      <span class="swatch" style={color ? `background:${colorToHex(color)}` : ''}></span>
      <span class="name">{scene?.name ?? `#${step.scene}`}</span>
      <button disabled={i === 0} onclick={() => move(i, i - 1)} aria-label="Move step {i + 1} up">↑</button>
      <button disabled={i === chaser.steps.length - 1} onclick={() => move(i, i + 1)} aria-label="Move step {i + 1} down">↓</button>
      <button onclick={() => updateSteps((steps) => steps.filter((_, j) => j !== i))} aria-label="Remove step {i + 1}">×</button>
    </li>
  {:else}
    <li class="muted">No steps yet: add some scenes.</li>
  {/each}
</ol>
<div class="row">
  <button class="primary" disabled={!hasScenes} onclick={() => (picking = f.id)}>Add scenes…</button>
  {#if !hasScenes}<span class="muted">Create a scene first.</span>
  {:else if chaser.steps.length > 1}<span class="muted">Drag the steps or use the arrows to order them.</span>{/if}
</div>

{#if picking === f.id}
  <FunctionPickerDialog
    title="Add scenes to {f.name}"
    only={['scene']}
    empty="There are no scenes yet."
    confirmLabel={(n) => `Add ${n} step${n === 1 ? '' : 's'}`}
    onconfirm={addScenes}
    onclose={() => (picking = null)}
  />
{/if}

<style>
  .steps { list-style: none; margin: 0 0 8px; padding: 0; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  li { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-bottom: 1px solid var(--border); }
  li:last-child { border-bottom: none; }
  li[draggable='true'] { cursor: grab; user-select: none; }
  li.over { outline: 2px dashed var(--accent); outline-offset: -2px; }
  .current { background: color-mix(in srgb, var(--accent) 20%, transparent); }
  .number { min-width: 2.5em; font-variant-numeric: tabular-nums; }
  .swatch { width: 14px; height: 14px; border-radius: 4px; border: 1px solid var(--border); flex: none; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
