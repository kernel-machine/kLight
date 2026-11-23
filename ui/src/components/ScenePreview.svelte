<script lang="ts">
  // Fixtures driven by a scene and the value of each channel, computed by
  // the engine's own compilation (groups, palettes and fans resolved),
  // next to what is on the output right now.
  import { app, client } from '../lib/app.svelte';
  import { scenePreview } from '../lib/core';
  import type { PreviewChannel } from '../lib/protocol';

  let { functionId }: { functionId: number } = $props();
  const show = $derived(app.s.show!);
  const fixtures = $derived(scenePreview(show, functionId));
  const universes = $derived([...new Set(fixtures.map((f) => f.universe))].sort((a, b) => a - b));
  const universeKey = $derived(universes.join(','));

  $effect(() => {
    const list = universeKey ? universeKey.split(',').map(Number) : [];
    if (!list.length) return;
    return client.watchDmx(list);
  });

  function live(universe: number, address: number | null): number | null {
    if (address === null) return null;
    return app.s.dmx.get(universe)?.[address - 1] ?? null;
  }
  const pct = (v: number) => `${Math.round((v / 255) * 100)}%`;
  const fine = (c: PreviewChannel) => (c.fine_value !== null ? ((c.value << 8) | c.fine_value) : null);
</script>

<h3>Fixtures in this scene ({fixtures.length})</h3>
{#if !fixtures.length}
  <p class="muted">No fixture is driven by this scene yet.</p>
{/if}
<div class="cards">
  {#each fixtures as f (f.fixture)}
    <div class="card">
      <div class="head">
        <b>{f.fixture_name}</b>
        <span class="muted">U{f.universe} · {f.address}</span>
      </div>
      <table>
        <thead><tr><th>Ch</th><th>Channel</th><th>Scene</th><th>Now</th></tr></thead>
        <tbody>
          {#each f.channels as c, i (i)}
            {@const now = live(f.universe, c.address)}
            <tr class:differs={now !== null && now !== c.value}>
              <td class="num">{c.address ?? '—'}{c.fine_address !== null ? `/${c.fine_address}` : ''}</td>
              <td>{c.channel_name}</td>
              <td class="value">
                <span class="bar"><span style="width:{pct(c.value)}"></span></span>
                <span class="num" title={fine(c) !== null ? `16-bit value ${fine(c)} of 65535` : ''}>{c.value}</span>
                {#if c.fine_value !== null}<span class="fine" title="Fine channel {c.fine_address}">fine {c.fine_value}</span>{/if}
              </td>
              <td class="num now">{now ?? '—'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/each}
</div>
<p class="muted small">"Scene" is the target value at the end of the fade; "Now" is the current output. Rows that differ are highlighted.</p>

<style>
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 8px; }
  .card { border: 1px solid var(--border); border-radius: var(--radius); padding: 6px; background: var(--panel-2); }
  .head { display: flex; justify-content: space-between; padding: 2px 4px 6px; }
  table { font-size: 12px; }
  td, th { padding: 2px 6px; }
  .num { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .value { display: flex; align-items: center; gap: 6px; }
  .bar { flex: 1; height: 6px; background: var(--border); border-radius: 3px; overflow: hidden; min-width: 40px; }
  .bar span { display: block; height: 100%; background: var(--accent); }
  .fine { font-size: 10px; color: var(--muted); white-space: nowrap; }
  tr.differs .now { color: var(--warn); font-weight: 700; }
  .small { font-size: 12px; }
</style>
