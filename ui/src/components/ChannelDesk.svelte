<script lang="ts">
  // All 512 channels of a universe, each editable on its own. A moved
  // fader overrides the channel (intensity still obeys GM and blackout)
  // until it is released.
  import { app, client, run } from '../lib/app.svelte';
  import { channelMap, UNIVERSE_SIZE } from '../lib/show';

  const show = $derived(app.s.show!);
  let universe = $state(app.s.show?.universes[0]?.id ?? 1);
  let filter = $state<'all' | 'patched' | 'set'>('all');

  $effect(() => {
    return client.watchDmx([universe]);
  });

  const map = $derived(channelMap(show, universe));
  const output = $derived(app.s.dmx.get(universe) ?? []);
  const overrides = $derived(
    new Map((app.s.runtime?.state.channels ?? []).filter((c) => c.universe === universe).map((c) => [c.address, c.value])),
  );
  /** Values being dragged, shown immediately (the engine confirms them). */
  let local = $state(new Map<number, number>());
  const addresses = $derived(
    Array.from({ length: UNIVERSE_SIZE }, (_, i) => i + 1).filter((a) =>
      filter === 'patched' ? map[a - 1] !== null : filter === 'set' ? overrides.has(a) : true,
    ),
  );

  // One command per channel per animation frame while dragging.
  const pending = new Map<number, number | null>();
  let raf = 0;
  function send(address: number, value: number | null) {
    pending.set(address, value);
    if (value === null) local.delete(address);
    else local.set(address, value);
    local = new Map(local);
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      const u = universe;
      for (const [a, v] of pending) run({ type: 'set_channel', universe: u, address: a, value: v });
      pending.clear();
    });
  }
  // Drop local values once the engine reports them.
  $effect(() => {
    const ov = overrides;
    let changed = false;
    for (const [a, v] of local) {
      if (ov.get(a) === v) {
        local.delete(a);
        changed = true;
      }
    }
    if (changed) local = new Map(local);
  });

  function valueOf(a: number): number {
    return local.get(a) ?? overrides.get(a) ?? output[a - 1] ?? 0;
  }
</script>

<div class="toolbar row">
  <label class="row">Universe
    <select bind:value={universe}>
      {#each show.universes as u (u.id)}<option value={u.id}>{u.id}: {u.name}</option>{/each}
    </select>
  </label>
  <div class="row">
    <button class:active={filter === 'all'} onclick={() => (filter = 'all')}>All 512</button>
    <button class:active={filter === 'patched'} onclick={() => (filter = 'patched')}>Patched</button>
    <button class:active={filter === 'set'} onclick={() => (filter = 'set')}>Set here ({overrides.size})</button>
  </div>
  <button disabled={!overrides.size} onclick={() => run({ type: 'clear_channels', universe })}>Release universe</button>
  <span class="muted">Moving a fader takes the channel over; × gives it back to scenes and programmer.</span>
</div>

<div class="strips">
  {#each addresses as a (a)}
    {@const info = map[a - 1]}
    {@const set = overrides.has(a) || local.has(a)}
    {@const v = valueOf(a)}
    <div
      class="strip"
      class:set
      class:patched={info !== null}
      class:alt={info !== null && info.fixture_index % 2 === 1}
      class:first={info?.offset === 0}
      title={info ? `${info.fixture_name} — ${info.channel_name}` : 'Not patched'}
    >
      <div class="addr">{a}</div>
      <div class="name">{info ? (info.offset === 0 ? info.fixture_name : '') : ''}</div>
      <div class="ch">{info?.channel_name ?? '—'}</div>
      <input
        type="range"
        min="0"
        max="255"
        step="1"
        value={v}
        aria-label="Channel {a}"
        oninput={(e) => send(a, Number(e.currentTarget.value))}
      />
      <input
        class="num"
        type="number"
        min="0"
        max="255"
        value={v}
        aria-label="Channel {a} value"
        onchange={(e) => send(a, Math.max(0, Math.min(255, Math.round(Number(e.currentTarget.value) || 0))))}
      />
      <button class="release" disabled={!set} title="Release" onclick={() => send(a, null)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toolbar { margin-bottom: 10px; }
  .strips { display: grid; grid-template-columns: repeat(auto-fill, minmax(56px, 1fr)); gap: 4px; }
  .strip { display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 4px 2px; border-radius: var(--radius); background: var(--panel); border: 1px solid var(--border); min-width: 0; }
  .strip.patched { background: color-mix(in srgb, var(--accent) 8%, var(--panel)); }
  .strip.patched.alt { background: color-mix(in srgb, var(--ok) 8%, var(--panel)); }
  .strip.first { border-left: 3px solid var(--accent); }
  .strip.set { border-color: var(--warn); box-shadow: inset 0 0 0 1px var(--warn); }
  .addr { font-weight: 700; font-variant-numeric: tabular-nums; font-size: 12px; }
  .name, .ch { font-size: 9px; color: var(--muted); width: 100%; text-align: center; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; height: 11px; }
  .name { color: var(--text); }
  input[type='range'] { writing-mode: vertical-lr; direction: rtl; height: 120px; width: 24px; }
  .num { width: 48px; padding: 2px; text-align: center; font-size: 12px; }
  .release { padding: 0 8px; font-size: 12px; }
</style>
