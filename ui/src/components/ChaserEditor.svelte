<script lang="ts">
  // Chaser contents: step times, run mode and the ordered list of steps
  // (scenes, effects and collections).
  import { app, notify, run } from '../lib/app.svelte';
  import { editQueue } from '../lib/queue';
  import { FUNCTION_CATEGORIES, chaserStepTypes } from '../lib/show';
  import TimeSelect from './TimeSelect.svelte';
  import FunctionIcon from './FunctionIcon.svelte';
  import FunctionPickerDialog from './FunctionPickerDialog.svelte';
  import type { Chaser, ChaserStep, Function as Fn } from '../lib/protocol';

  let {
    f,
    chaser,
    picking = $bindable(),
  }: {
    f: Fn;
    chaser: Chaser;
    /** Chaser whose "add steps" dialog is open. */
    picking: number | null;
  } = $props();

  const show = $derived(app.s.show!);
  const position = $derived(app.s.runtime?.state.active.find((a) => a.id === f.id)?.position);
  const stepTypes = chaserStepTypes();
  const hasPlayable = $derived(show.functions.some((x) => stepTypes.includes(x.kind.type)));
  const hasEffects = $derived(chaser.steps.some((s) => (sceneOrEffect(s)?.kind.type ?? 'scene') !== 'scene'));

  // Edits are queued and each one is applied to the latest version of the
  // chaser: quick successive clicks must not overwrite each other.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function update(change: (chaser: Chaser) => Partial<Chaser> | null) {
    const id = f.id;
    enqueue(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.kind.type !== 'chaser') return;
      const changed = change(current.kind);
      if (!changed) return;
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, ...changed } } });
    });
  }

  function addSteps(ids: number[]) {
    picking = null;
    update((c) => ({ steps: [...c.steps, ...ids.map((scene) => ({ scene, hold: null, fade: null }))] }));
  }

  /**
   * An edit of steps named by their position, which only means something on the
   * list the user acted on (`seen`): it is dropped if the steps have changed.
   */
  function editPositions(seen: ChaserStep[], change: (steps: ChaserStep[]) => ChaserStep[]) {
    update((c) => {
      if (JSON.stringify(c.steps) !== JSON.stringify(seen)) {
        notify('The steps changed in the meantime: check the list and try again.', 'warning');
        return null;
      }
      return { steps: change([...c.steps]) };
    });
  }

  /** Moves the step at `from` so that it ends up at `to`. */
  function move(seen: ChaserStep[], from: number, to: number) {
    const inList = (i: number) => i >= 0 && i < seen.length;
    if (from === to || !inList(from) || !inList(to)) return;
    editPositions(seen, (steps) => {
      steps.splice(to, 0, ...steps.splice(from, 1));
      return steps;
    });
  }

  const remove = (seen: ChaserStep[], index: number) => editPositions(seen, (steps) => steps.filter((_, i) => i !== index));

  // ---- drag & drop (mouse only: touch screens use the arrows)
  let dragged = $state.raw<{ index: number; seen: ChaserStep[] } | null>(null);
  let over = $state<number | null>(null);

  function drop(to: number) {
    if (dragged) move(dragged.seen, dragged.index, to);
    dragged = null;
    over = null;
  }

  // `step.scene` is the function the step plays, scene or effect.
  const sceneOrEffect = (step: ChaserStep) => show.functions.find((x) => x.id === step.scene);
  const typeLabel = (f: Fn) => FUNCTION_CATEGORIES.find((c) => c.id === f.kind.type)?.short;
</script>

<div class="row">
  <TimeSelect label="Step" value={chaser.hold} onchange={(hold) => update(() => ({ hold }))} />
  <TimeSelect label="Fade" zero value={chaser.fade} onchange={(fade) => update(() => ({ fade }))} />
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
    {@const played = sceneOrEffect(step)}
    <li
      class:current={position === i}
      class:over={over === i && dragged?.index !== i}
      draggable="true"
      ondragstart={(e) => { dragged = { index: i, seen: chaser.steps }; e.dataTransfer?.setData('text/plain', `s${i}`); }}
      ondragend={() => { dragged = null; over = null; }}
      ondragover={(e) => { if (dragged) { e.preventDefault(); over = i; } }}
      ondragleave={() => { if (over === i) over = null; }}
      ondrop={(e) => { e.preventDefault(); drop(i); }}
    >
      <span class="grip muted" aria-hidden="true">⠿</span>
      <span class="number">{i + 1}.</span>
      {#if played}<FunctionIcon f={played} />{/if}
      <span class="name">{#if played}{played.name}{:else}#{step.scene} <span class="muted">(missing function)</span>{/if}</span>
      {#if played && played.kind.type !== 'scene'}<span class="muted type">{typeLabel(played)}</span>{/if}
      <button disabled={i === 0} onclick={() => move(chaser.steps, i, i - 1)} aria-label="Move step {i + 1} up">↑</button>
      <button disabled={i === chaser.steps.length - 1} onclick={() => move(chaser.steps, i, i + 1)} aria-label="Move step {i + 1} down">↓</button>
      <button onclick={() => remove(chaser.steps, i)} aria-label="Remove step {i + 1}">×</button>
    </li>
  {:else}
    <li class="muted">No steps yet: add some scenes, effects or collections.</li>
  {/each}
</ol>
<div class="row">
  <button class="primary" disabled={!hasPlayable} onclick={() => (picking = f.id)}>Add steps…</button>
  {#if !hasPlayable}<span class="muted">Create a scene, an effect or a collection first.</span>
  {:else if chaser.steps.length > 1}<span class="muted">Order the steps with the arrows, or drag them with the mouse.</span>{/if}
</div>
{#if hasEffects}<p class="muted">The fade crossfades scenes: effects start and stop at once.</p>{/if}

{#if picking === f.id}
  <FunctionPickerDialog
    title="Add steps to {f.name}"
    only={stepTypes}
    empty="There is nothing a step can play yet."
    confirmLabel={(n) => `Add ${n} step${n === 1 ? '' : 's'}`}
    onconfirm={addSteps}
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
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .type { font-size: 12px; }
</style>
