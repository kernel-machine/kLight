<script lang="ts">
  // Output values of a universe; fixtures are separated by color bands and
  // every channel says what it is.
  import { app, client } from '../lib/app.svelte';
  import { channelMap, groupColor, groupsOf, GROUP_COLORS } from '../lib/show';

  let { onclose }: { onclose: () => void } = $props();
  let universe = $state(app.s.show?.universes[0]?.id ?? 1);
  let patchedOnly = $state(false);

  $effect(() => {
    return client.watchDmx([universe]);
  });
  const show = $derived(app.s.show!);
  const data = $derived(app.s.dmx.get(universe) ?? []);
  const map = $derived(channelMap(show, universe));
  const band = (fixture_index: number) => GROUP_COLORS[fixture_index % GROUP_COLORS.length];
  const cells = $derived(Array.from({ length: 512 }, (_, i) => i).filter((i) => !patchedOnly || map[i]));
</script>

<div class="monitor panel">
  <div class="row">
    <h2>DMX monitor</h2>
    <select bind:value={universe} aria-label="Monitor universe">
      {#each show.universes as u (u.id)}
        <option value={u.id}>{u.id}: {u.name}</option>
      {/each}
    </select>
    <label class="row"><input type="checkbox" bind:checked={patchedOnly} /> Patched only</label>
    <span class="spacer"></span>
    <button onclick={onclose}>Close</button>
  </div>
  <div class="cells">
    {#each cells as i (i)}
      {@const v = data[i] ?? 0}
      {@const info = map[i]}
      {@const groups = info ? groupsOf(show, info.fixture) : []}
      <div
        class="cell"
        class:patched={info}
        class:start={info?.offset === 0}
        style="--v:{v / 255}; --band:{info ? band(info.fixture_index) : 'transparent'}"
        title="{i + 1}: {info ? `${info.fixture_name} — ${info.channel_name} (${info.attribute})` : 'not patched'} = {v}"
      >
        <div class="top">
          <span class="addr">{i + 1}</span>
          {#if groups[0]}<span class="g" style="background:{groupColor(show, groups[0].id)}"></span>{/if}
        </div>
        <span class="fixture">{info?.offset === 0 ? info.fixture_name : ''}</span>
        <span class="ch">{info?.channel_name ?? ''}</span>
        <span class="v">{v}</span>
        <div class="bar"></div>
      </div>
    {/each}
  </div>
</div>

<style>
  .monitor { position: fixed; left: 12px; right: 132px; bottom: 44px; max-height: 60vh; overflow: auto; z-index: 40; box-shadow: 0 8px 32px #000a; }
  .spacer { flex: 1; }
  .cells { display: grid; grid-template-columns: repeat(auto-fill, minmax(64px, 1fr)); gap: 3px; margin-top: 8px; }
  .cell { position: relative; background: var(--panel-2); border-radius: 3px; padding: 2px 3px 5px; display: flex; flex-direction: column; font-variant-numeric: tabular-nums; border-top: 4px solid var(--band); overflow: hidden; }
  .cell.patched { background: color-mix(in srgb, var(--band) 10%, var(--panel-2)); }
  .cell.start { box-shadow: inset 3px 0 0 var(--band); }
  .top { display: flex; justify-content: space-between; align-items: center; }
  .addr { font-size: 10px; color: var(--muted); }
  .g { width: 7px; height: 7px; border-radius: 50%; }
  .fixture { font-size: 9px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; height: 11px; }
  .ch { font-size: 9px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; height: 11px; }
  .v { font-size: 14px; text-align: center; }
  .bar { position: absolute; left: 0; bottom: 0; height: 3px; width: calc(var(--v) * 100%); background: var(--accent); }
</style>
