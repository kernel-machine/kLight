<script lang="ts">
  // Settings of one fixture: only what applies to its type is shown.
  import { app, run } from '../../lib/app.svelte';
  import { conflictsWith, definitionOf, footprint, groupColor, groupsOf, hasColorMixing, hasPosition, modeOf } from '../../lib/show';
  import type { Orientation, PatchedFixture } from '../../lib/protocol';

  let { id, onclose }: { id: number; onclose: () => void } = $props();
  const show = $derived(app.s.show!);
  const f = $derived(show.fixtures.find((x) => x.id === id));
  const def = $derived(f ? definitionOf(show, f) : undefined);
  const mode = $derived(f ? modeOf(show, f) : undefined);
  const moving = $derived(f ? hasPosition(show, f) : false);
  const colorMixing = $derived(f ? hasColorMixing(show, f) : false);

  function patch(changes: Partial<PatchedFixture>, force = false) {
    if (f) run({ type: 'patch_fixture', fixture: { ...f, ...changes }, force });
  }
  function orient(key: keyof Orientation, value: boolean) {
    if (f) patch({ orientation: { ...f.orientation, [key]: value } }, true);
  }
  function changeMode(name: string) {
    if (!f || !def) return;
    const size = def.modes.find((m) => m.name === name)?.channels.length ?? 0;
    const clash = conflictsWith(show, f.universe, f.address, size, f.id);
    if (clash.length && !window.confirm(`The new mode overlaps fixtures ${clash.join(', ')}. Patch anyway?`)) return;
    patch({ mode: name }, clash.length > 0);
  }
</script>

{#if f && def && mode}
  <section class="panel col">
    <div class="row head">
      <h2>{f.name}</h2>
      <span class="muted">{def.manufacturer} — {def.model}</span>
      <span class="spacer"></span>
      <button onclick={onclose} aria-label="Close">×</button>
    </div>
    <div class="grid">
      <label>Name <input value={f.name} onchange={(e) => patch({ name: e.currentTarget.value })} /></label>
      <label>Mode
        <select value={f.mode} onchange={(e) => changeMode(e.currentTarget.value)}>
          {#each def.modes as m (m.name)}<option value={m.name}>{m.name} ({m.channels.length} ch)</option>{/each}
        </select>
      </label>
      <label>Universe
        <select value={f.universe} onchange={(e) => patch({ universe: Number(e.currentTarget.value) })}>
          {#each show.universes as u (u.id)}<option value={u.id}>{u.id}: {u.name}</option>{/each}
        </select>
      </label>
      <label>Address <input type="number" min="1" max="512" value={f.address} onchange={(e) => patch({ address: Number(e.currentTarget.value) })} /></label>
    </div>
    <div class="muted">Channels {f.address}–{f.address + footprint(show, f) - 1} · {Math.max(1, mode.heads.length)} head{mode.heads.length > 1 ? 's' : ''}</div>

    {#if groupsOf(show, f.id).length}
      <div class="row">
        {#each groupsOf(show, f.id) as g (g.id)}<span class="badge" style="border-color:{groupColor(show, g.id)}; color:{groupColor(show, g.id)}">{g.name}</span>{/each}
      </div>
    {/if}

    {#if moving}
      <h3>Orientation</h3>
      <div class="row">
        <label class="row"><input type="checkbox" checked={f.orientation.invert_pan} onchange={(e) => orient('invert_pan', e.currentTarget.checked)} /> Invert pan</label>
        <label class="row"><input type="checkbox" checked={f.orientation.invert_tilt} onchange={(e) => orient('invert_tilt', e.currentTarget.checked)} /> Invert tilt</label>
        <label class="row"><input type="checkbox" checked={f.orientation.swap_pan_tilt} onchange={(e) => orient('swap_pan_tilt', e.currentTarget.checked)} /> Swap pan/tilt</label>
        <label class="row"><input type="checkbox" checked={f.orientation.upside_down} onchange={(e) => orient('upside_down', e.currentTarget.checked)} /> Upside down</label>
      </div>
      <div class="muted small">Range: pan {(mode.physical ?? def.physical).pan_max_deg}°, tilt {(mode.physical ?? def.physical).tilt_max_deg}°</div>
    {/if}

    {#if colorMixing}
      <h3>Color calibration</h3>
      <div class="row">
        <span class="muted small">{f.calibration ? 'Custom correction matrix' : 'None (factory colors)'}</span>
        {#if f.calibration}<button onclick={() => patch({ calibration: null }, true)}>Reset</button>{/if}
      </div>
    {/if}

    <h3>Channels</h3>
    <table class="channels">
      <tbody>
        {#each mode.channels as ci, i (i)}
          {@const ch = def.channels[ci]}
          <tr><td class="muted">{f.address + i}</td><td>{ch?.name}</td><td class="muted">{ch?.attribute}</td></tr>
        {/each}
      </tbody>
    </table>
    <div class="row">
      <button class="danger" onclick={() => { run({ type: 'unpatch_fixture', id: f.id }); onclose(); }}>Remove fixture</button>
    </div>
  </section>
{/if}

<style>
  .head h2 { margin: 0; }
  .spacer { flex: 1; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 8px; }
  .grid label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  .grid input, .grid select { color: var(--text); }
  .small { font-size: 12px; }
  .channels td { padding: 3px 8px; font-size: 13px; }
</style>
