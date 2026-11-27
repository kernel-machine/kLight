<script lang="ts">
  // Chaser contents, laid out as the scene editor: a header (name, transport to
  // try it step by step, live preview, rarely used actions in a menu), the
  // chaser's times and play mode with their real durations, a timeline of the
  // steps as long as they last, and the ordered list of steps (scenes, effects
  // and collections), each with its own hold and fade if it has some. While it
  // runs, the current step is marked on both and clicking a step jumps to it.
  import { tick } from 'svelte';
  import { app, client, notify, pref, run, setPref } from '../lib/app.svelte';
  import { livePreview } from '../lib/livePreview';
  import { editQueue } from '../lib/queue';
  import { chaserTimes } from '../lib/core';
  import { FUNCTION_CATEGORIES, chaserStepTypes, colorToHex, formatTime, functionColor } from '../lib/show';
  import TimeSelect from './TimeSelect.svelte';
  import StepTime from './StepTime.svelte';
  import FunctionIcon from './FunctionIcon.svelte';
  import FunctionPickerDialog from './FunctionPickerDialog.svelte';
  import type { Chaser, ChaserStep, Function as Fn } from '../lib/protocol';

  let {
    f,
    chaser,
    picking = $bindable(),
    onduplicate,
    ondelete,
  }: {
    f: Fn;
    chaser: Chaser;
    /** Chaser whose "add steps" dialog is open. */
    picking: number | null;
    onduplicate: () => void;
    ondelete: () => void;
  } = $props();

  const show = $derived(app.s.show!);
  const active = $derived(app.s.runtime?.state.active.find((a) => a.id === f.id));
  const running = $derived(active !== undefined);
  const position = $derived(active?.position ?? null);
  const bpm = $derived(app.s.runtime?.state.tempo.bpm ?? 120);
  const times = $derived(chaserTimes(show, f.id, bpm));
  const stepTypes = chaserStepTypes();
  const hasPlayable = $derived(show.functions.some((x) => stepTypes.includes(x.kind.type)));
  const hasEffects = $derived(chaser.steps.some((s) => (sceneOrEffect(s)?.kind.type ?? 'scene') !== 'scene'));

  // Edits are queued and each one is applied to the latest version of the
  // chaser: quick successive clicks must not overwrite each other.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function update(change: (chaser: Chaser) => Partial<Chaser> | null) {
    const id = f.id;
    return enqueue(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.kind.type !== 'chaser') return;
      const changed = change(current.kind);
      if (!changed) return;
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, ...changed } } });
    });
  }

  /**
   * Renamed in the same queue as the other edits, on the latest chaser: a copy
   * taken before steps just added came back would drop them.
   */
  function rename(name: string) {
    const id = f.id;
    enqueue(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.name === name) return;
      return run({ type: 'set_function', function: { ...current, name } });
    });
  }

  function addSteps(ids: number[]) {
    picking = null;
    update((c) => ({ steps: [...c.steps, ...ids.map((scene) => ({ scene, hold: null, fade: null }))] }));
  }

  /**
   * An edit of steps named by their position, which only means something on the
   * list the user acted on (`seen`): it is dropped if the steps have moved,
   * been added or removed since. Times changed meanwhile do not count: two
   * quick time edits on different steps are both kept.
   */
  const order = (steps: ChaserStep[]) => JSON.stringify(steps.map((s) => s.scene));
  function editPositions(seen: ChaserStep[], change: (steps: ChaserStep[]) => ChaserStep[]) {
    return update((c) => {
      if (order(c.steps) !== order(seen)) {
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
    return editPositions(seen, (steps) => {
      steps.splice(to, 0, ...steps.splice(from, 1));
      return steps;
    });
  }

  const remove = (seen: ChaserStep[], index: number) => editPositions(seen, (steps) => steps.filter((_, i) => i !== index));
  const duplicateStep = (seen: ChaserStep[], index: number) =>
    editPositions(seen, (steps) => {
      const s = steps[index];
      if (s) steps.splice(index + 1, 0, { ...s });
      return steps;
    });
  const setStep = (seen: ChaserStep[], index: number, change: Partial<ChaserStep>) =>
    editPositions(seen, (steps) => steps.map((s, i) => (i === index ? { ...s, ...change } : s)));

  // ---- drag & drop with the mouse; ↑/↓ on a step's handle.
  let dragged = $state.raw<{ index: number; seen: ChaserStep[] } | null>(null);
  let over = $state<number | null>(null);

  function drop(to: number) {
    if (dragged) move(dragged.seen, dragged.index, to);
    dragged = null;
    over = null;
  }
  async function key(e: KeyboardEvent, i: number) {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    const to = e.key === 'ArrowUp' ? i - 1 : i + 1;
    const moving = move(chaser.steps, i, to);
    if (!moving) return;
    await moving;
    await tick();
    // Keep the moved step in hand, unless the focus went elsewhere meanwhile.
    const at = document.activeElement;
    if (at === handle || at === document.body || at === null) list?.querySelectorAll<HTMLElement>('button.grip')[to]?.focus();
  }
  let list = $state<HTMLElement>();

  // ---- trying it: jump to a step while it runs; pick one to preview it.
  /** The step looked at while stopped (previewed on the lights if asked). */
  let selected = $state<number | null>(null);
  $effect(() => {
    void f.id;
    selected = null;
    menuOpen = false;
  });
  const n = $derived(chaser.steps.length);
  function goTo(index: number) {
    if (!n) return;
    const i = ((index % n) + n) % n;
    if (running) run({ type: 'chaser_go_to', id: f.id, index: i });
    else selected = i;
  }
  /** One step back or on, from the current one (or the one looked at). */
  const step = (d: number) => goTo((running ? (position ?? 0) : (selected ?? (d > 0 ? -1 : 0))) + d);
  const toggleRun = () => run(running ? { type: 'stop_function', id: f.id } : { type: 'start_function', id: f.id });

  let wantsPreview = $state(pref('chaser.preview', false));
  $effect(() => setPref('chaser.preview', wantsPreview));
  const canPreview = $derived(
    app.s.connection === 'connected' && app.s.permission === 'full' && app.s.runtime?.state.mode === 'Design',
  );
  /** The scene of the step looked at, previewed while the chaser is stopped. */
  const previewed = $derived.by(() => {
    if (!wantsPreview || !canPreview || running || selected === null) return null;
    const s = chaser.steps[selected];
    const played = s ? sceneOrEffect(s) : undefined;
    return played?.kind.type === 'scene' ? played.kind : null;
  });
  let failed = false;
  const preview = livePreview((command) => {
    client.command(command).catch((e: Error) => {
      if (!failed) notify(`Preview: ${e.message}`, 'error');
      failed = true;
    });
  });
  $effect(() => {
    if (previewed) preview.show(previewed.values, previewed.members);
    else preview.stop();
  });
  $effect(() => () => preview.stop());

  // ---- progress of the current step, for the timeline and the list. The
  // engine says which step plays; how far into it is counted here from when
  // that step began to show, for the eye only.
  let since = $state(performance.now());
  let now = $state(performance.now());
  $effect(() => {
    void position;
    since = performance.now();
  });
  $effect(() => {
    if (!running) return;
    let frame = 0;
    const loop = () => {
      now = performance.now();
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  });
  const progress = $derived.by(() => {
    if (!running || position === null) return 0;
    const hold = times?.steps[position]?.hold_ms ?? 0;
    return hold > 0 ? Math.min(1, Math.max(0, (now - since) / hold)) : 1;
  });

  // ---- the header's menu of rarely used actions.
  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  const act = (fn: () => void) => () => {
    menuOpen = false;
    fn();
  };
  function outside(e: MouseEvent) {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }

  // `step.scene` is the function the step plays, scene or effect.
  const sceneOrEffect = (step: ChaserStep) => show.functions.find((x) => x.id === step.scene);
  const typeLabel = (f: Fn) => FUNCTION_CATEGORIES.find((c) => c.id === f.kind.type)?.short;
  const tint = (s: ChaserStep) => {
    const played = sceneOrEffect(s);
    const c = played ? functionColor(show, played) : null;
    return c ? colorToHex(c) : 'var(--muted)';
  };
  const seconds = (ms: number) => `${(ms / 1000).toLocaleString(undefined, { maximumFractionDigits: 2 })} s`;
  const RUN_MODES: Array<[Chaser['run_mode'], string]> = [['Loop', 'Loop'], ['SingleShot', 'Single shot'], ['PingPong', 'Ping-pong']];
</script>

<svelte:window onclick={outside} />

<!-- Header: name; trying it step by step; the rare actions in a menu. -->
<div class="head">
  <input class="name" value={f.name} aria-label="Chaser name" onchange={(e) => rename(e.currentTarget.value)} />
  <span class="badge">Chaser</span>
  <span class="spacer"></span>
  <div class="test" role="group" aria-label="Test">
    <button aria-label="Previous step" title="Previous step" disabled={!n} onclick={() => step(-1)}>
      <svg width="13" height="13" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 2v10M12 2 5 7l7 5z" fill="currentColor" stroke="currentColor" stroke-width="1.4"></path></svg>
    </button>
    {#if running}
      <button class="primary run" onclick={toggleRun}>Stop</button>
    {:else}
      <button class="run" disabled={!n} onclick={toggleRun}>Start</button>
    {/if}
    <button aria-label="Next step" title="Next step" disabled={!n} onclick={() => step(1)}>
      <svg width="13" height="13" viewBox="0 0 14 14" aria-hidden="true"><path d="M11 2v10M2 2l7 5-7 5z" fill="currentColor" stroke="currentColor" stroke-width="1.4"></path></svg>
    </button>
    <label class="live" title={canPreview ? 'While stopped, the step you pick is shown on the lights' : 'Only in Design mode, with full access'}>
      <input type="checkbox" bind:checked={wantsPreview} disabled={!canPreview} />
      Live preview on the lights
    </label>
  </div>
  <div class="menu-wrap" bind:this={menuWrap}>
    <button class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>⋯</button>
    {#if menuOpen}
      <div class="menu" role="menu" tabindex="-1" onkeydown={(e) => { if (e.key === 'Escape') menuOpen = false; }}>
        <button role="menuitem" onclick={act(onduplicate)}>Duplicate</button>
        <hr />
        <button role="menuitem" class="danger-text" onclick={act(ondelete)}>Delete chaser</button>
      </div>
    {/if}
  </div>
</div>

<!-- The chaser's times and play mode, with what they last at this tempo. -->
<section class="card settings" aria-label="Chaser settings">
  <TimeSelect label="Hold" value={chaser.hold} onchange={(hold) => update(() => ({ hold }))} />
  {#if times && 'beats' in chaser.hold}<span class="muted small">{seconds(times.hold_ms)} at {Math.round(bpm)} BPM</span>{/if}
  <TimeSelect label="Fade" zero value={chaser.fade} onchange={(fade) => update(() => ({ fade }))} />
  <label class="row">
    Mode
    <select value={chaser.run_mode} aria-label="Run mode" onchange={(e) => { const run_mode = e.currentTarget.value as Chaser['run_mode']; update(() => ({ run_mode })); }}>
      {#each RUN_MODES as [v, label] (v)}<option value={v}>{label}</option>{/each}
    </select>
  </label>
  <label class="row">
    Direction
    <select value={chaser.direction} aria-label="Direction" onchange={(e) => { const direction = e.currentTarget.value as Chaser['direction']; update(() => ({ direction })); }}>
      <option value="Forward">Forward</option><option value="Backward">Backward</option>
    </select>
  </label>
  <span class="spacer"></span>
  {#if times && n}<span class="muted small" title="Every step once">One pass: {seconds(times.pass_ms)}</span>{/if}
</section>

{#if n && times}
  <!-- One block per step, as wide as it lasts; the fade shaded at its start. -->
  <section class="card timeline" aria-label="Timeline">
    <div class="row">
      <h3>Timeline</h3>
      <span class="muted small">
        {#if running && position !== null}Playing step {position + 1} of {n}{:else}Stopped · click a step to {wantsPreview && canPreview ? 'preview' : 'pick'} it{/if}
      </span>
    </div>
    <div class="blocks">
      {#each chaser.steps as s, i (i)}
        {@const t = times.steps[i]}
        {@const played = sceneOrEffect(s)}
        {@const now_ = running && position === i}
        <button
          class="block"
          class:now={now_}
          class:picked={!running && selected === i}
          style="flex-grow:{Math.max(t?.hold_ms ?? 1, 1)}; --c:{tint(s)}"
          aria-label="Step {i + 1}: {played?.name ?? 'missing function'}"
          aria-current={now_ ? 'step' : undefined}
          onclick={() => goTo(i)}
        >
          {#if t && t.fade_ms > 0}<span class="fade" style="width:{Math.min(100, (t.fade_ms / Math.max(t.hold_ms, 1)) * 100)}%"></span>{/if}
          <span class="fill" style="width:{now_ ? progress * 100 : running && position !== null && i < position ? 100 : 0}%"></span>
          <span class="label">{i + 1}. {played?.name ?? `#${s.scene}`}</span>
          <span class="len">{t ? formatTime(t.hold) : ''}</span>
        </button>
      {/each}
    </div>
  </section>
{/if}

<section class="card" aria-label="Steps">
  <div class="row steps-head">
    <h3>Steps <span class="muted">· {n}</span></h3>
    <span class="spacer"></span>
    {#if n > 1}<span class="muted small">Drag ⠿ to reorder, or focus it and use ↑/↓</span>{/if}
  </div>
  <ol class="steps" aria-label="Chaser steps" bind:this={list}>
    {#each chaser.steps as s, i (i)}
      {@const played = sceneOrEffect(s)}
      {@const now_ = running && position === i}
      <li
        class:current={now_}
        class:picked={!running && selected === i}
        class:over={over === i && dragged?.index !== i}
        draggable="true"
        ondragstart={(e) => { dragged = { index: i, seen: chaser.steps }; e.dataTransfer?.setData('text/plain', `s${i}`); }}
        ondragend={() => { dragged = null; over = null; }}
        ondragover={(e) => { if (dragged) { e.preventDefault(); over = i; } }}
        ondragleave={() => { if (over === i) over = null; }}
        ondrop={(e) => { e.preventDefault(); drop(i); }}
      >
        {#if now_}<span class="bar" style="width:{progress * 100}%"></span>{/if}
        <button class="grip" aria-label="Move step {i + 1}" title="Drag, or ↑/↓ keys, to move it" onkeydown={(e) => key(e, i)}>⠿</button>
        <span class="number">{i + 1}.</span>
        <button class="pick" onclick={() => goTo(i)} title={running ? 'Jump to this step' : 'Pick this step'}>
          {#if played}<FunctionIcon f={played} />{/if}
          <span class="name">{#if played}{played.name}{:else}#{s.scene} <span class="muted">(missing function)</span>{/if}</span>
          {#if played && played.kind.type !== 'scene'}<span class="muted type">{typeLabel(played)}</span>{/if}
          {#if now_}<span class="playing">Playing</span>{/if}
        </button>
        <span class="time">Hold <StepTime label="Hold of step {i + 1}" own={s.hold} fallback={chaser.hold} onchange={(hold) => setStep(chaser.steps, i, { hold })} /></span>
        <span class="time">Fade <StepTime label="Fade of step {i + 1}" own={s.fade} fallback={chaser.fade} zero onchange={(fade) => setStep(chaser.steps, i, { fade })} /></span>
        <button class="icon" onclick={() => duplicateStep(chaser.steps, i)} aria-label="Duplicate step {i + 1}" title="Duplicate">⧉</button>
        <button class="icon" onclick={() => remove(chaser.steps, i)} aria-label="Remove step {i + 1}" title="Remove">×</button>
      </li>
    {:else}
      <li class="muted">No steps yet: add some scenes, effects or collections.</li>
    {/each}
  </ol>
  <div class="row">
    <button class="primary" disabled={!hasPlayable} onclick={() => (picking = f.id)}>Add steps…</button>
    {#if !hasPlayable}<span class="muted">Create a scene, an effect or a collection first.</span>
    {:else}<span class="muted small">Tick a folder in the dialog to add all of it, in order.</span>{/if}
  </div>
  {#if hasEffects}<p class="muted small">The fade crossfades scenes: effects start and stop at once.</p>{/if}
</section>

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
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .head .name { font-size: 18px; font-weight: 600; min-width: 0; flex: 0 1 280px; }
  .spacer { flex: 1; }
  .test { display: flex; align-items: stretch; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .test > button { border: none; border-radius: 0; min-height: 36px; padding: 0 12px; }
  .test > button + button { border-left: 1px solid var(--border); }
  .test .run { font-weight: 600; padding: 0 16px; }
  .test .live { display: flex; align-items: center; gap: 6px; padding: 0 12px; border-left: 1px solid var(--border); font-size: 13px; cursor: pointer; }
  .menu-wrap { position: relative; }
  .more { width: 38px; min-height: 36px; padding: 0; font-size: 18px; line-height: 1; }
  .menu { position: absolute; right: 0; top: calc(100% + 6px); z-index: 20; min-width: 220px; display: flex; flex-direction: column; padding: 6px; background: var(--panel-2); border: 1px solid var(--border); border-radius: var(--radius); box-shadow: 0 10px 28px rgb(0 0 0 / 45%); }
  .menu button { text-align: left; border: none; background: none; padding: 8px 10px; }
  .menu button:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .menu hr { width: 100%; border: none; border-top: 1px solid var(--border); margin: 4px 0; }
  .danger-text { color: var(--danger, #e5484d); }
  .card { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px 12px; background: var(--panel-2); margin-top: 14px; min-width: 0; }
  .card h3 { margin: 0; font-size: 15px; }
  .small { font-size: 12px; }
  .settings { display: flex; align-items: center; gap: 10px 16px; flex-wrap: wrap; }
  .timeline .row { margin-bottom: 8px; align-items: baseline; }
  .blocks { display: flex; gap: 4px; height: 58px; }
  .block { position: relative; flex: 1 1 0; min-width: 28px; overflow: hidden; padding: 0; text-align: left; border: 2px solid transparent; border-radius: 8px; background: color-mix(in srgb, var(--c) 22%, var(--panel)); }
  .block.now { border-color: var(--text); }
  .block.picked { border-color: var(--accent); }
  .block .fill { position: absolute; inset: 0 auto 0 0; background: color-mix(in srgb, var(--c) 55%, transparent); }
  .block .fade { position: absolute; inset: 0 auto 0 0; background: linear-gradient(90deg, rgb(255 255 255 / 0%), rgb(255 255 255 / 16%)); }
  .block .label, .block .len { position: relative; display: block; padding: 0 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .block .label { padding-top: 7px; font-weight: 600; font-size: 13px; }
  .block .len { font-size: 11px; opacity: 0.8; }
  .steps-head { align-items: baseline; margin-bottom: 8px; }
  .steps { list-style: none; margin: 0 0 8px; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  li { position: relative; display: flex; align-items: center; gap: 8px; padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--panel); overflow: hidden; }
  li[draggable='true'] { user-select: none; }
  li.over { outline: 2px dashed var(--accent); outline-offset: -2px; }
  li.current { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, var(--panel)); }
  li.picked { border-color: var(--accent); }
  .bar { position: absolute; left: 0; bottom: 0; height: 3px; background: var(--accent); }
  .grip { cursor: grab; border: none; background: none; padding: 2px 4px; color: var(--muted); }
  .number { min-width: 2em; font-variant-numeric: tabular-nums; color: var(--muted); }
  .pick { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; border: none; background: none; padding: 2px 0; text-align: left; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .type { font-size: 12px; }
  .playing { font-size: 11px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--accent); }
  .time { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); }
  .icon { width: 32px; padding: 4px 0; }
</style>
