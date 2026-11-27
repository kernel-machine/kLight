<script lang="ts">
  // Movement effect editor, laid out as the scene and chaser editors: a header
  // (name, Start/Stop to try it on the lights, rare actions in a menu), the
  // drawing (edited directly: drag the center and the edges), where the lights
  // reach and which head is which, then the controls. The drawing follows the
  // values while dragging; the engine receives them when the control is
  // released. Edits and renames go through one queue, each applied to the
  // latest version of the function.
  import { untrack } from 'svelte';
  import { app, notify, run } from '../lib/app.svelte';
  import { editQueue } from '../lib/queue';
  import { draftSync } from '../lib/draftSync';
  import { movementReach } from '../lib/core';
  import TimeSelect from './TimeSelect.svelte';
  import type { Function as Fn, MovementEffect, MovementMirror, MovementShape, RunMode } from '../lib/protocol';
  import MovementPreview, { LIMITS } from './MovementPreview.svelte';

  let { f, onduplicate, ondelete }: { f: Fn; onduplicate: () => void; ondelete: () => void } = $props();
  const show = $derived(app.s.show!);
  const source = $derived(f.kind.type === 'movement_effect' ? f.kind : null);
  const running = $derived((app.s.runtime?.state.active ?? []).some((a) => a.id === f.id));
  let draft = $state<MovementEffect | null>(null);

  // Follow the engine's version without undoing edits still on their way.
  const sync = draftSync<MovementEffect>();
  const strip = (kind: Fn['kind']): MovementEffect | null => {
    if (kind.type !== 'movement_effect') return null;
    const { type: _type, ...rest } = kind;
    return $state.snapshot(rest) as MovementEffect;
  };
  $effect(() => {
    const incoming = source ? strip(source) : null;
    const id = f.id;
    if (!incoming) return;
    untrack(() => {
      const shown = sync.incoming(id, incoming, draft ? ($state.snapshot(draft) as MovementEffect) : null);
      if (shown) draft = shown;
    });
  });

  // Heads, ranges and clipping from the core, for the draft as it is drawn.
  const reach = $derived(draft ? movementReach(show, $state.snapshot(draft) as MovementEffect) : null);
  const moving = $derived(reach?.heads.filter((h) => h.moves).length ?? 0);

  const enqueue = editQueue((e) => notify(String(e), 'error'));
  /** The function as the engine has it now. */
  const latest = (id: number) => app.s.show?.functions.find((x) => x.id === id);

  function commit() {
    if (!draft) return;
    const kind = $state.snapshot(draft) as MovementEffect;
    sync.sent(kind);
    const id = f.id;
    enqueue(async () => {
      const current = latest(id);
      if (!current || current.kind.type !== 'movement_effect') return;
      const ok = await run({ type: 'set_function', function: { ...current, kind: { type: 'movement_effect', ...kind } } });
      const now = latest(id);
      const engine = now ? strip(now.kind) : null;
      if (!engine || f.id !== id) return;
      const local = draft ? ($state.snapshot(draft) as MovementEffect) : null;
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

  // A relative shape is placed by an offset from where the scenes put the heads.
  const sliders = $derived([
    { key: 'center_pan', label: draft?.relative ? 'Offset pan' : 'Center pan', min: -LIMITS.pan, max: LIMITS.pan },
    { key: 'center_tilt', label: draft?.relative ? 'Offset tilt' : 'Center tilt', min: -LIMITS.tilt, max: LIMITS.tilt },
    { key: 'width', label: 'Width', min: 0, max: LIMITS.width },
    { key: 'height', label: 'Height', min: 0, max: LIMITS.height },
    { key: 'rotation', label: 'Rotation', min: -180, max: 180 },
  ] as const);

  const MIRRORS: Array<[MovementMirror, string]> = [['none', 'None'], ['alternate', 'Every second head'], ['halves', 'Second half of the group']];
  const RUN_MODES: Array<[RunMode, string]> = [['Loop', 'Loop'], ['SingleShot', 'Single shot'], ['PingPong', 'Ping-pong']];

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

  const toggleRun = () => run(running ? { type: 'stop_function', id: f.id } : { type: 'start_function', id: f.id });
  const range = (limit: number | null) => (limit === null ? null : `±${Math.round(limit)}°`);

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
  <input class="name" value={f.name} aria-label="Movement name" onchange={(e) => rename(e.currentTarget.value)} />
  <span class="badge">Movement</span>
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
        <button role="menuitem" class="danger-text" onclick={act(ondelete)}>Delete movement</button>
      </div>
    {/if}
  </div>
</div>

{#if draft && reach}
  <div class="editor">
    <div class="col">
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

    <div class="col">
      <label class="row">Group
        <select value={draft.group} aria-label="Movement group" onchange={(e) => { draft!.group = Number(e.currentTarget.value); commit(); }}>
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
      <div class="phase">
        <span>Heads</span>
        <div class="seg" role="group" aria-label="Phase presets">
          {#each PHASES as p (p.value)}
            <button class:active={draft.phase_spread === p.value} title={p.title} onclick={() => setPhase(p.value)}>{p.label}</button>
          {/each}
        </div>
      </div>
      <label class="slider">
        <span>Phase spread</span>
        <input type="range" min="0" max="360" step="0.5" bind:value={draft.phase_spread} onchange={commit} />
        <input type="number" min="0" max="360" step="0.5" bind:value={draft.phase_spread} onchange={commit} class="num" />°
      </label>
      <label class="row">Mirrored
        <select value={draft.mirror} aria-label="Mirrored heads" title="Heads that draw the shape mirrored left to right" onchange={(e) => { draft!.mirror = e.currentTarget.value as MovementMirror; commit(); }}>
          {#each MIRRORS as [v, label] (v)}<option value={v}>{label}</option>{/each}
        </select>
      </label>
      <div class="row">
        <TimeSelect label="Cycle" value={draft.period} onchange={(period) => { draft!.period = period; commit(); }} />
        <button onclick={() => { draft!.direction = draft!.direction === 'Forward' ? 'Backward' : 'Forward'; commit(); }}>
          Direction: {draft.direction}
        </button>
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
      <label class="row" title="The shape is added to the position the scenes give each head; the center becomes an offset from there">
        <input type="checkbox" checked={draft.relative} onchange={(e) => { draft!.relative = e.currentTarget.checked; commit(); }} />
        Relative to the position of the scenes
      </label>
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
  .editor { display: grid; grid-template-columns: minmax(280px, 1.2fr) 1fr; gap: 12px; align-items: start; }
  @media (max-width: 1100px) { .editor { grid-template-columns: 1fr; } }
  .slider { display: grid; grid-template-columns: 100px 1fr 72px auto; align-items: center; gap: 6px; }
  .num { width: 72px; }
  .shapes button { text-transform: capitalize; }
  .phase { display: grid; grid-template-columns: 100px 1fr; align-items: center; gap: 6px; }
  .seg { display: flex; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; width: fit-content; }
  .seg button { border: none; border-radius: 0; }
  .seg button + button { border-left: 1px solid var(--border); }
  .notice { margin: 8px 0 0; font-size: 13px; }
  .notice.warn { color: var(--warn); }
  .small { font-size: 12px; margin: 8px 0 0; }
  .heads { display: flex; flex-wrap: wrap; gap: 4px 12px; list-style: none; padding: 0; margin: 8px 0 0; font-size: 12px; }
  .heads b { display: inline-grid; place-items: center; min-width: 18px; height: 18px; border-radius: 9px; background: var(--text); color: var(--bg, #000); font-size: 11px; margin-right: 2px; }
  .heads li:first-child b { background: var(--ok); }
  .heads li.still { opacity: 0.6; }
</style>
