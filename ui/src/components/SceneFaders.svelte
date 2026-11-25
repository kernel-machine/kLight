<script lang="ts">
  // All the channels of the fixtures a scene drives, one row of faders. The
  // values come from the engine's own compilation (groups, palettes and fans
  // resolved); moving a fader stores that channel in the scene.
  import { app, client } from '../lib/app.svelte';
  import { channelPresets, sceneChannels } from '../lib/core';
  import { hasPresets, presetAt } from '../lib/channelIcon';
  import FaderStrip from './FaderStrip.svelte';
  import PresetPicker from './PresetPicker.svelte';
  import type { ChannelRef, SceneChannel, SceneFixtureChannels } from '../lib/protocol';

  let {
    functionId,
    onset,
  }: {
    functionId: number;
    /** Stores a channel in the scene (`null` releases it); resolves when applied. */
    onset: (fixture: number, channel: ChannelRef, value: number | null) => Promise<unknown>;
  } = $props();

  const show = $derived(app.s.show!);
  const fixtures = $derived(sceneChannels(show, functionId));
  const universeKey = $derived([...new Set(fixtures.map((f) => f.universe))].sort((a, b) => a - b).join(','));

  $effect(() => {
    const list = universeKey ? universeKey.split(',').map(Number) : [];
    if (!list.length) return;
    return client.watchDmx(list);
  });

  const keyOf = (fixture: number, c: SceneChannel) => `${functionId}:${fixture}:${JSON.stringify(c.channel)}`;

  /** Values being dragged or waiting for the engine, shown immediately. */
  let local = $state<Record<string, number>>({});

  // A change is stored when the fader is let go: one undo step per move.
  async function commit(fixture: number, c: SceneChannel, value: number) {
    const key = keyOf(fixture, c);
    local[key] = value;
    await onset(fixture, c.channel, value);
    // Moved again in the meantime: that value is still on its way.
    if (local[key] === value) delete local[key];
  }

  function release(fixture: number, c: SceneChannel) {
    delete local[keyOf(fixture, c)];
    onset(fixture, c.channel, null);
  }

  function live(universe: number, address: number | null): number | null {
    if (address === null) return null;
    return app.s.dmx.get(universe)?.[address - 1] ?? null;
  }

  const presetsOf = (f: SceneFixtureChannels, c: SceneChannel) => (c.address === null ? [] : channelPresets(show, f.universe, c.address));

  let picker = $state<{ fixture: SceneFixtureChannels; channel: SceneChannel; x: number; top: number; bottom: number } | null>(null);
  function openPicker(e: MouseEvent, fixture: SceneFixtureChannels, channel: SceneChannel) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    picker = { fixture, channel, x: r.left, top: r.top, bottom: r.bottom };
  }
</script>

<h3>Fixtures in this scene ({fixtures.length})</h3>
{#if !fixtures.length}
  <p class="muted">No fixture is driven by this scene yet.</p>
{:else}
  <div class="strips" role="group" aria-label="Scene channels">
    {#each fixtures as f, n (f.fixture)}
      {#each f.channels as c, i (JSON.stringify(c.channel))}
        {@const key = keyOf(f.fixture, c)}
        {@const v = local[key] ?? c.value}
        {@const presets = presetsOf(f, c)}
        {@const choosable = hasPresets(presets)}
        {@const preset = choosable ? presets[presetAt(presets, v)] : undefined}
        <div class="slot">
          <FaderStrip
            address={c.address ?? 'V'}
            fixture={i === 0 ? f.fixture_name : ''}
            channel={preset?.name ?? c.channel_name}
            attribute={c.attribute}
            value={v}
            label="{f.fixture_name}: {c.channel_name}"
            title="{f.fixture_name} — {c.channel_name}{preset ? `: ${preset.name}` : ''}{c.locked ? ' (service channel: not stored in scenes)' : ''}"
            alt={n % 2 === 1}
            first={i === 0}
            set={c.origin === 'manual' || key in local}
            scene={c.origin === 'value'}
            unset={c.origin === 'unset' && !(key in local)}
            disabled={c.locked}
            now={live(f.universe, c.address)}
            onchoose={choosable ? (e) => openPicker(e, f, c) : undefined}
            oninput={(value) => (local[key] = value)}
            onslideend={(value) => commit(f.fixture, c, value)}
            onentry={(value) => commit(f.fixture, c, value)}
            onrelease={() => release(f.fixture, c)}
          />
        </div>
      {/each}
    {/each}
  </div>
  <p class="muted small">
    Blue border: set by the values above. Orange border: set by hand (× gives it back). Dimmed: not set by this scene.
    The small number is the output right now.
  </p>
{/if}

{#if picker}
  {@const p = picker}
  <PresetPicker
    title="{p.fixture.fixture_name} — {p.channel.channel_name}"
    presets={presetsOf(p.fixture, p.channel)}
    value={local[keyOf(p.fixture.fixture, p.channel)] ?? p.channel.value}
    x={p.x}
    top={p.top}
    bottom={p.bottom}
    onpick={(value) => commit(p.fixture.fixture, p.channel, value)}
    onclose={() => (picker = null)}
  />
{/if}

<style>
  /* One row only: it scrolls sideways to reach every channel. */
  .strips { display: flex; gap: 4px; overflow-x: auto; padding-bottom: 8px; }
  .slot { flex: 0 0 60px; display: grid; }
  .small { font-size: 12px; }
</style>
