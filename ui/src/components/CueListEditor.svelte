<script lang="ts">
  // Cue list editor, laid out as the other FX editors: a header (name, the
  // transport to run the show from here, rarely used actions in a menu), where
  // the list is and what comes next, then the cues. Each cue's number, name,
  // times, follow and move in black are edited in its row; its values are
  // shown on demand and updated from the programmer. New cues are recorded from
  // the programmer after the current one, numbered between their neighbours.
  // Edits are queued, each applied to the latest version of the list.
  import { app, notify, run } from '../lib/app.svelte';
  import { editQueue } from '../lib/queue';
  import { cueListTimes, insertCueNumber, updateCueValues } from '../lib/core';
  import { describeSceneValue, describeTarget, formatSeconds } from '../lib/show';
  import TimeField from './TimeField.svelte';
  import type { Cue, CueList, Function as Fn, SceneValue, TimeValue } from '../lib/protocol';

  let {
    f,
    list,
    onduplicate,
    ondelete,
  }: { f: Fn; list: CueList; onduplicate: () => void; ondelete: () => void } = $props();

  const show = $derived(app.s.show!);
  const active = $derived(app.s.runtime?.state.active.find((a) => a.id === f.id));
  const running = $derived(active !== undefined);
  const paused = $derived(active?.paused ?? false);
  const current = $derived(active?.position ?? null);
  const bpm = $derived(app.s.runtime?.state.tempo.bpm ?? 120);
  const times = $derived(cueListTimes(show, f.id, bpm));
  const programmer = $derived(app.s.runtime?.state.programmer ?? []);
  const fromProgrammer = (): SceneValue[] => programmer.map((e) => ({ target: e.target, value: { type: 'explicit', value: e.value } }));
  const n = $derived(list.cues.length);
  /** The cue GO plays next. */
  const next = $derived(current === null ? (n ? 0 : null) : current + 1 < n ? current + 1 : null);

  // ---- edits, queued and applied to the latest list.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function update(change: (list: CueList) => Partial<CueList> | null) {
    const id = f.id;
    return enqueue(() => {
      const latest = app.s.show?.functions.find((x) => x.id === id);
      if (!latest || latest.kind.type !== 'cue_list') return;
      const changed = change(latest.kind);
      if (!changed) return;
      return run({ type: 'set_function', function: { ...latest, kind: { ...latest.kind, ...changed } } });
    });
  }
  function rename(name: string) {
    const id = f.id;
    enqueue(() => {
      const latest = app.s.show?.functions.find((x) => x.id === id);
      if (!latest || latest.name === name) return;
      return run({ type: 'set_function', function: { ...latest, name } });
    });
  }

  /**
   * An edit of cues named by their place, which only means something on the
   * list the user acted on: dropped if cues were added, removed or moved since.
   * Other edits of the cues meanwhile do not count.
   */
  const order = (cues: Cue[]) => JSON.stringify(cues.map((c) => [c.number, c.values.length]));
  const shape = (cues: Cue[]) => cues.length;
  function editCues(seen: Cue[], change: (cues: Cue[]) => Cue[], strict = true) {
    return update((l) => {
      const same = strict ? order(l.cues) === order(seen) : shape(l.cues) === shape(seen);
      if (!same) {
        notify('The cues changed in the meantime: check the list and try again.', 'warning');
        return null;
      }
      return { cues: change(l.cues.map((c) => ({ ...c }))) };
    });
  }
  /** One cue's own fields (number, name, times...): kept along with other such edits. */
  const setCue = (seen: Cue[], i: number, change: (c: Cue) => Partial<Cue>) =>
    editCues(seen, (cues) => cues.map((c, j) => (j === i ? { ...c, ...change(c) } : c)), false);
  const setTiming = (seen: Cue[], i: number, key: 'delay' | 'fade_in' | 'fade_out', t: TimeValue) =>
    setCue(seen, i, (c) => ({ timing: { ...c.timing, [key]: t } }));

  /** Where a new cue goes: after the one playing, else at the end. */
  const insertAt = () => (current !== null && current < n ? current + 1 : n);
  function newCue(cues: Cue[], at: number, values: SceneValue[]): Cue {
    const number = insertCueNumber(cues, at);
    const before = cues[at - 1];
    return {
      number,
      name: `Cue ${number}`,
      values,
      timing: before ? { ...before.timing } : { delay: { ms: 0 }, fade_in: { ms: 2000 }, fade_out: { ms: 2000 } },
      follow: { type: 'manual' },
      move_in_black: null,
    };
  }
  function record() {
    const values = fromProgrammer();
    if (!values.length) return;
    const at = insertAt();
    editCues(list.cues, (cues) => {
      cues.splice(at, 0, newCue(cues, at, values));
      return cues;
    });
  }
  const updateFromProgrammer = (seen: Cue[], i: number) => {
    const incoming = fromProgrammer();
    if (incoming.length) setCue(seen, i, (c) => ({ values: updateCueValues(show, c.values, incoming) }));
  };
  const duplicateCue = (seen: Cue[], i: number) =>
    editCues(seen, (cues) => {
      const c = cues[i];
      if (c) cues.splice(i + 1, 0, { ...c, number: insertCueNumber(cues, i + 1), name: `${c.name} copy` });
      return cues;
    });
  const removeCue = (seen: Cue[], i: number) => editCues(seen, (cues) => cues.filter((_, j) => j !== i));
  /** Moved one place; it takes a number between its new neighbours. */
  function move(seen: Cue[], i: number, to: number) {
    if (to < 0 || to >= seen.length) return;
    editCues(seen, (cues) => {
      const [c] = cues.splice(i, 1);
      if (!c) return cues;
      const number = insertCueNumber(cues, to);
      cues.splice(to, 0, { ...c, number });
      return cues;
    });
  }
  const renumber = () =>
    editCues(list.cues, (cues) => cues.map((c, i) => ({ ...c, number: i + 1, name: c.name === `Cue ${c.number}` ? `Cue ${i + 1}` : c.name })));

  // ---- transport
  const go = () => run({ type: 'cue_go', id: f.id });
  const back = () => run({ type: 'cue_back', id: f.id });
  const pause = () => run({ type: 'cue_pause', id: f.id });
  const stop = () => run({ type: 'stop_function', id: f.id });
  const goTo = (index: number) => run({ type: 'cue_go_to', id: f.id, index });

  // ---- progress of the current cue: its fade, then the wait for an
  // automatic follow. The engine says which cue plays; how far into it is
  // counted here from when it began to show, for the eye only.
  let since = $state(performance.now());
  let now = $state(performance.now());
  $effect(() => {
    void current;
    since = performance.now();
  });
  $effect(() => {
    if (!running || paused) return;
    let frame = 0;
    const loop = () => {
      now = performance.now();
      frame = requestAnimationFrame(loop);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  });
  const progress = $derived.by(() => {
    const t = current === null ? undefined : times[current];
    if (!running || !t) return null;
    const elapsed = now - since;
    const fade = t.delay_ms + t.fade_in_ms;
    return {
      fade: fade > 0 ? Math.min(1, elapsed / fade) : 1,
      follow: t.follow_ms === null ? null : Math.max(0, t.follow_ms - elapsed),
    };
  });

  // ---- values shown on demand
  let open = $state<Set<number>>(new Set());
  const toggle = (i: number) => {
    const next = new Set(open);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    open = next;
  };

  // ---- the header's menu
  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  $effect(() => {
    void f.id;
    menuOpen = false;
    open = new Set();
  });
  const act = (fn: () => void) => () => {
    menuOpen = false;
    fn();
  };
  function outside(e: MouseEvent) {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }
  const seconds = (ms: number) => `${(ms / 1000).toLocaleString(undefined, { maximumFractionDigits: 1 })} s`;
  const label = (i: number | null) => {
    const c = i === null ? undefined : list.cues[i];
    return c ? `${c.number} ${c.name}` : '';
  };
  const MIB: Array<[string, boolean | null]> = [['List', null], ['On', true], ['Off', false]];
</script>

<svelte:window onclick={outside} />

<div class="head">
  <input class="name" value={f.name} aria-label="Cue list name" onchange={(e) => rename(e.currentTarget.value)} />
  <span class="badge">Cue list</span>
  <span class="spacer"></span>
  <div class="transport" role="group" aria-label="Transport">
    <button onclick={back} disabled={!running || !current} title="Previous cue">Back</button>
    <button class="primary go" onclick={go} disabled={next === null} title={next === null ? 'No cue after this one' : `Go: ${label(next)}`}>GO</button>
    <button onclick={pause} disabled={!running}>{paused ? 'Resume' : 'Pause'}</button>
    <button onclick={stop} disabled={!running}>Stop</button>
  </div>
  <div class="menu-wrap" bind:this={menuWrap}>
    <button class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>⋯</button>
    {#if menuOpen}
      <div class="menu" role="menu" tabindex="-1" onkeydown={(e) => { if (e.key === 'Escape') menuOpen = false; }}>
        <button role="menuitem" onclick={act(onduplicate)}>Duplicate</button>
        <button role="menuitem" disabled={!n} onclick={act(renumber)}>Renumber cues 1, 2, 3…</button>
        <hr />
        <button role="menuitem" class="danger-text" onclick={act(ondelete)}>Delete cue list</button>
      </div>
    {/if}
  </div>
</div>

<!-- Where the list is, and what GO does. -->
<section class="card status" aria-label="Status" aria-live="polite">
  <div class="now">
    {#if running && current !== null}
      <span class="muted small">{paused ? 'Paused on' : 'Playing'}</span>
      <b data-current>Cue {label(current)}</b>
      {#if progress}
        <div class="bar" title="Fade"><i style="width:{progress.fade * 100}%"></i></div>
      {/if}
    {:else}
      <span class="muted small">Stopped</span>
      <b>{n ? 'GO starts the first cue' : 'No cues yet'}</b>
    {/if}
  </div>
  <div class="next">
    <span class="muted small">Next</span>
    {#if next !== null}
      <b data-next>Cue {label(next)}</b>
      {#if progress?.follow !== null && progress?.follow !== undefined}<span class="small">automatically in {seconds(progress.follow)}</span>{/if}
    {:else}
      <b class="muted">{n ? 'End of the list' : '—'}</b>
    {/if}
  </div>
  <label class="row mib" title="A head that is dark in a cue moves in advance to its position in the next cue where it lights up">
    <input type="checkbox" checked={list.move_in_black} onchange={(e) => { const move_in_black = e.currentTarget.checked; update(() => ({ move_in_black })); }} />
    Move in black
  </label>
</section>

<section class="card cues" aria-label="Cues">
  <div class="row top">
    <h3>Cues</h3>
    <span class="muted small">{n} cue{n === 1 ? '' : 's'} · times in seconds, or "2 beats"</span>
    <span class="spacer"></span>
    <button class="primary" disabled={!programmer.length} title={programmer.length ? 'The values of the programmer, as a new cue after the current one' : 'Set values in the Simple Desk first'} onclick={record}>
      Record cue{programmer.length ? ` (${programmer.length} values)` : ''}
    </button>
  </div>
  {#if n}
    <div class="grid header" aria-hidden="true">
      <span></span><span>#</span><span>Name</span><span>Fade in</span><span>Fade out</span><span>Delay</span><span>Follow</span><span>Move in black</span><span>Values</span><span></span>
    </div>
    <ol class="list">
      {#each list.cues as c, i (i)}
        {@const t = times[i]}
        <li class:current={running && current === i} class:next={next === i} aria-label="Cue {c.number}">
          <div class="grid">
            <button class="go-to" aria-label="Go to cue {c.number}" title="Go to this cue" onclick={() => goTo(i)}>▶</button>
            <input class="num" type="number" step="0.1" min="0" aria-label="Number" value={c.number} onchange={(e) => { const number = Number(e.currentTarget.value); if (Number.isFinite(number)) setCue(list.cues, i, () => ({ number })); }} />
            <input class="cue-name" aria-label="Name" value={c.name} onchange={(e) => { const name = e.currentTarget.value; setCue(list.cues, i, () => ({ name })); }} />
            <TimeField label="Fade in" value={c.timing.fade_in} ms={t?.fade_in_ms} onchange={(v) => setTiming(list.cues, i, 'fade_in', v)} />
            <TimeField label="Fade out" value={c.timing.fade_out} ms={t?.fade_out_ms} onchange={(v) => setTiming(list.cues, i, 'fade_out', v)} />
            <TimeField label="Delay" value={c.timing.delay} ms={t?.delay_ms} onchange={(v) => setTiming(list.cues, i, 'delay', v)} />
            <span class="follow">
              <select aria-label="Follow" value={c.follow.type} onchange={(e) => { const auto = e.currentTarget.value === 'auto'; setCue(list.cues, i, () => ({ follow: auto ? { type: 'auto', after: { ms: 5000 } } : { type: 'manual' } })); }}>
                <option value="manual">Wait for GO</option>
                <option value="auto">Go on after</option>
              </select>
              {#if c.follow.type === 'auto'}
                {@const follow = c.follow}
                <TimeField label="Go on after" value={follow.after} ms={t?.follow_ms ?? undefined} onchange={(after) => setCue(list.cues, i, () => ({ follow: { type: 'auto', after } }))} />
              {/if}
            </span>
            <select aria-label="Move in black" value={String(c.move_in_black)} onchange={(e) => { const v = e.currentTarget.value; setCue(list.cues, i, () => ({ move_in_black: v === 'null' ? null : v === 'true' })); }}>
              {#each MIB as [name, v] (name)}<option value={String(v)}>{v === null ? `As the list (${list.move_in_black ? 'on' : 'off'})` : name}</option>{/each}
            </select>
            <button class="values" aria-expanded={open.has(i)} onclick={() => toggle(i)}>{c.values.length} value{c.values.length === 1 ? '' : 's'} {open.has(i) ? '▾' : '▸'}</button>
            <span class="actions">
              <button aria-label="Update cue {c.number} from the programmer" title={programmer.length ? 'Add the programmer to this cue (same values replaced)' : 'Set values in the Simple Desk first'} disabled={!programmer.length} onclick={() => updateFromProgrammer(list.cues, i)}>Update</button>
              <button aria-label="Duplicate cue {c.number}" title="Duplicate" onclick={() => duplicateCue(list.cues, i)}>⧉</button>
              <button aria-label="Move cue {c.number} up" title="Move up" disabled={i === 0} onclick={() => move(list.cues, i, i - 1)}>↑</button>
              <button aria-label="Move cue {c.number} down" title="Move down" disabled={i === n - 1} onclick={() => move(list.cues, i, i + 1)}>↓</button>
              <button aria-label="Remove cue {c.number}" title="Remove" onclick={() => removeCue(list.cues, i)}>×</button>
            </span>
          </div>
          {#if open.has(i)}
            <ul class="cue-values" aria-label="Values of cue {c.number}">
              {#each c.values as v, k (k)}
                <li><span class="muted">{describeTarget(show, v.target)}</span> {describeSceneValue(show, v)}</li>
              {:else}
                <li class="muted">No values: a dark cue.</li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ol>
  {:else}
    <p class="muted empty">Set the lights in the Simple Desk, then <b>Record cue</b>. Each recording becomes the next cue.</p>
  {/if}
</section>

<style>
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .head .name { font-size: 18px; font-weight: 600; min-width: 0; flex: 0 1 280px; }
  .spacer { flex: 1; }
  .transport { display: flex; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .transport > button { border: none; border-radius: 0; min-height: 36px; padding: 0 12px; }
  .transport > button + button { border-left: 1px solid var(--border); }
  .transport .go { font-size: 16px; font-weight: 700; padding: 0 22px; }
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
  .status { display: grid; grid-template-columns: 1fr 1fr auto; gap: 16px; align-items: center; }
  @media (max-width: 800px) { .status { grid-template-columns: 1fr; } }
  .status .now, .status .next { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .bar { height: 4px; background: var(--border); border-radius: 2px; overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--ok); }
  .top { align-items: center; gap: 10px; margin-bottom: 8px; }
  .cues { overflow-x: auto; }
  .grid { display: grid; grid-template-columns: 30px 64px minmax(120px, 1fr) 80px 80px 80px minmax(150px, auto) 130px 90px auto; gap: 6px; align-items: center; min-width: 980px; }
  .grid.header { font-size: 12px; color: var(--muted); padding: 0 6px 4px; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .list > li { padding: 4px 6px; border: 1px solid transparent; border-radius: var(--radius); }
  .list > li.current { background: color-mix(in srgb, var(--ok) 16%, transparent); border-color: color-mix(in srgb, var(--ok) 50%, transparent); }
  .list > li.next { border-color: color-mix(in srgb, var(--accent) 45%, transparent); border-style: dashed; }
  .num { width: 64px; }
  .cue-name { min-width: 0; }
  .follow { display: flex; gap: 4px; align-items: center; }
  .go-to { width: 30px; padding: 0; }
  .values { white-space: nowrap; }
  .actions { display: flex; gap: 2px; }
  .actions button { padding: 2px 7px; }
  .cue-values { margin: 6px 0 2px 40px; padding: 0; list-style: none; font-size: 13px; display: grid; gap: 2px; }
  .empty { margin: 4px 0; }
</style>
