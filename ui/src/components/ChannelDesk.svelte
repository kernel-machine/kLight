<script lang="ts">
  // All 512 channels of a universe, each editable on its own. A moved
  // fader overrides the channel (intensity still obeys GM and blackout)
  // until it is released.
  import { app, client, run } from '../lib/app.svelte';
  import { channelMap, UNIVERSE_SIZE } from '../lib/show';
  import FaderStrip from './FaderStrip.svelte';
  import FixtureGroup from './FixtureGroup.svelte';
  import { fixtureRuns } from '../lib/fixtureRuns';
  import PresetPicker from './PresetPicker.svelte';
  import { channelPresets } from '../lib/core';
  import { hasPresets, presetAt } from '../lib/channelIcon';

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
  const runs = $derived(fixtureRuns(addresses, map));

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

  // Channels with named ranges (color wheel, gobos, shutter...) open a list
  // of them; picking one sets the channel, so its fader jumps there.
  let picker = $state<{ address: number; x: number; top: number; bottom: number } | null>(null);
  function openPicker(e: MouseEvent, address: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    picker = { address, x: r.left, top: r.top, bottom: r.bottom };
  }
  const pickerChannel = $derived(picker ? map[picker.address - 1] : null);
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
  {#each runs as r (r.key)}
    <FixtureGroup name={r.name} patched={r.patched} alt={r.alt} wrap>
      {#each r.addresses as a (a)}
        {@const info = map[a - 1]}
        {@const set = overrides.has(a) || local.has(a)}
        {@const v = valueOf(a)}
        {@const presets = info ? channelPresets(show, universe, a) : []}
        {@const choosable = hasPresets(presets)}
        {@const preset = choosable ? presets[presetAt(presets, v)] : undefined}
        <div class="slot">
          <FaderStrip
            address={a}
            channel={preset?.name ?? info?.channel_name ?? '—'}
            attribute={info?.attribute ?? null}
            value={v}
            label="Channel {a}"
            subject="channel {a}"
            title={info ? `${info.fixture_name} — ${info.channel_name}${preset ? `: ${preset.name}` : ''}` : 'Not patched'}
            patched={info !== null}
            alt={info !== null && info.fixture_index % 2 === 1}
            first={info?.offset === 0}
            {set}
            onchoose={choosable ? (e) => openPicker(e, a) : undefined}
            oninput={(value) => send(a, value)}
            onentry={(value) => send(a, value)}
            onrelease={() => send(a, null)}
          />
        </div>
      {/each}
    </FixtureGroup>
  {/each}
</div>

{#if picker && pickerChannel}
  <PresetPicker
    title="{pickerChannel.fixture_name} — {pickerChannel.channel_name}"
    presets={channelPresets(show, universe, picker.address)}
    value={valueOf(picker.address)}
    x={picker.x}
    top={picker.top}
    bottom={picker.bottom}
    onpick={(value) => picker && send(picker.address, value)}
    onclose={() => (picker = null)}
  />
{/if}

<style>
  .toolbar { margin-bottom: 10px; }
  .strips { display: flex; flex-wrap: wrap; gap: 10px 6px; }
  .slot { flex: 0 0 60px; display: grid; }
</style>
