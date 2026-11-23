<script lang="ts">
  // Selection-based programmer: pick groups or fixtures, then an intention.
  import { app, run } from '../lib/app.svelte';
  import { capabilityNames, describeTarget, describeValue, groupFixtures, hexToColor } from '../lib/show';
  import type { AttributeValue, FanMode, PaletteKind, Target } from '../lib/protocol';
  import ChannelDesk from '../components/ChannelDesk.svelte';
  import { pref, setPref } from '../lib/app.svelte';

  let view = $state<'intentions' | 'channels'>(pref('desk.view', 'intentions'));
  $effect(() => setPref('desk.view', view));

  const show = $derived(app.s.show!);
  const programmer = $derived(app.s.runtime?.state.programmer ?? []);
  let selection = $state<Target[]>([]);
  let intensity = $state(1);
  let color = $state('#ff0000');
  let pan = $state(0);
  let tilt = $state(0);
  let fan = $state(0);
  let fanMode = $state<FanMode>('linear');
  let zoom = $state(20);
  let strobeHz = $state(5);
  let paletteKind = $state<PaletteKind>('Color');

  const key = (t: Target) => JSON.stringify(t);
  const isSel = (t: Target) => selection.some((s) => key(s) === key(t));
  function toggle(t: Target) {
    selection = isSel(t) ? selection.filter((s) => key(s) !== key(t)) : [...selection, t];
  }

  const selectedFixtures = $derived.by(() => {
    const ids = new Set<number>();
    for (const t of selection) {
      if (t.type === 'group') {
        const g = show.groups.find((x) => x.id === t.id);
        if (g) groupFixtures(show, g).forEach((f) => ids.add(f.id));
      } else ids.add(t.id);
    }
    return show.fixtures.filter((f) => ids.has(f.id));
  });
  const gobos = $derived(capabilityNames(show, selectedFixtures, 'Gobo'));
  const prisms = $derived(capabilityNames(show, selectedFixtures, 'Prism'));

  async function apply(value: AttributeValue) {
    for (const target of selection) await run({ type: 'programmer_set', target, value });
  }

  /** Position; on groups with a spread, a fan resolved by the core in group order. */
  async function applyPosition() {
    for (const target of selection) {
      const value: AttributeValue =
        target.type === 'group' && fan !== 0
          ? { type: 'position_fan', pan, tilt, pan_spread: fan, tilt_spread: 0, mode: fanMode }
          : { type: 'position', pan, tilt };
      await run({ type: 'programmer_set', target, value });
    }
  }

  /** Applying a palette copies its intention for each selected fixture. */
  async function applyPalette(id: number) {
    const p = show.palettes.find((x) => x.id === id);
    if (!p) return;
    for (const f of selectedFixtures) {
      const values = p.per_fixture.find((e) => e.fixture === f.id)?.values ?? p.generic;
      for (const value of values) await run({ type: 'programmer_set', target: { type: 'fixture', id: f.id }, value });
    }
  }

  async function saveScene() {
    const name = window.prompt('Scene name', 'New scene');
    if (name) await run({ type: 'store_programmer_as_scene', id: null, name });
  }

  async function savePalette(existing: number | null) {
    const name = existing === null ? window.prompt('Palette name', `${paletteKind} palette`) : show.palettes.find((p) => p.id === existing)?.name;
    if (name) await run({ type: 'store_programmer_in_palette', id: existing, name, kind: paletteKind });
  }
</script>

<div class="row tabs">
  <button class:active={view === 'intentions'} onclick={() => (view = 'intentions')}>Selection &amp; intentions</button>
  <button class:active={view === 'channels'} onclick={() => (view = 'channels')}>DMX channels</button>
</div>

{#if view === 'channels'}
  <ChannelDesk />
{:else}
<div class="desk">
  <section class="panel">
    <h2>Selection</h2>
    <h3>Groups</h3>
    <div class="row">
      {#each show.groups as g (g.id)}
        {@const t: Target = { type: 'group', id: g.id }}
        <button class="chip" class:selected={isSel(t)} onclick={() => toggle(t)}>{g.name}</button>
      {:else}<span class="muted">No groups yet</span>{/each}
    </div>
    <h3>Fixtures</h3>
    <div class="row">
      {#each show.fixtures as f (f.id)}
        {@const t: Target = { type: 'fixture', id: f.id }}
        <button class="chip" class:selected={isSel(t)} onclick={() => toggle(t)}>{f.name}</button>
      {/each}
    </div>
    <div class="row" style="margin-top:8px"><button onclick={() => (selection = [])}>Clear selection</button></div>
  </section>

  <section class="panel controls" class:disabled={!selection.length}>
    <h2>Intentions</h2>
    <label class="row">Intensity <input type="range" min="0" max="1" step="0.01" bind:value={intensity} onchange={() => apply({ type: 'intensity', level: intensity })} /> {Math.round(intensity * 100)}%</label>
    <div class="row">
      <button onclick={() => apply({ type: 'intensity', level: 1 })}>Full</button>
      <button onclick={() => apply({ type: 'intensity', level: 0 })}>Out</button>
    </div>
    <h3>Color</h3>
    <div class="row">
      {#each ['#ff0000', '#ff8000', '#ffff00', '#00ff00', '#00ffff', '#0000ff', '#ff00ff', '#ffffff'] as c}
        <button class="swatch" style="background:{c}" title={c} onclick={() => apply({ type: 'color', color: hexToColor(c) })}></button>
      {/each}
      <input type="color" bind:value={color} onchange={() => apply({ type: 'color', color: hexToColor(color) })} />
    </div>
    <h3>Position</h3>
    <label class="row">Pan <input type="range" min="-270" max="270" step="0.5" bind:value={pan} /> {pan}°</label>
    <label class="row">Tilt <input type="range" min="-135" max="135" step="0.5" bind:value={tilt} /> {tilt}°</label>
    <label class="row">Fan <input type="range" min="-180" max="180" step="1" bind:value={fan} /> {fan}°
      <select bind:value={fanMode}><option value="linear">linear</option><option value="from_center">from center</option></select>
    </label>
    <button onclick={applyPosition}>Set position</button>
    <h3>Beam</h3>
    <div class="row">
      {#each gobos as g}<button onclick={() => apply({ type: 'gobo', name: g })}>{g}</button>{/each}
    </div>
    <div class="row">
      {#each prisms as p}<button onclick={() => apply({ type: 'prism', name: p })}>{p}</button>{/each}
    </div>
    <label class="row">Zoom <input type="range" min="1" max="60" bind:value={zoom} onchange={() => apply({ type: 'zoom', degrees: zoom })} /> {zoom}°</label>
    <div class="row">
      <label class="row">Strobe <input type="number" min="0.5" max="30" step="0.5" bind:value={strobeHz} style="width:70px" /> Hz</label>
      <button onclick={() => apply({ type: 'strobe', strobe: { type: 'Hz', hz: strobeHz } })}>Strobe</button>
      <button onclick={() => apply({ type: 'strobe', strobe: { type: 'Open' } })}>Open</button>
    </div>
    <h3>Palettes</h3>
    <div class="row">
      {#each show.palettes as p (p.id)}<button onclick={() => applyPalette(p.id)}>{p.name}</button>{:else}<span class="muted">None</span>{/each}
    </div>
  </section>

  <section class="panel">
    <h2>Programmer</h2>
    {#each programmer as e, i (i)}
      <div class="row entry"><span>{describeTarget(show, e.target)}</span><span class="muted">{describeValue(e.value)}</span></div>
    {:else}<span class="muted">Empty</span>{/each}
    <div class="row" style="margin-top:10px">
      <button onclick={() => run({ type: 'programmer_clear' })}>Clear</button>
      <button class="primary" disabled={!programmer.length} onclick={saveScene}>Save as scene</button>
    </div>
    <h3>Update palette</h3>
    <div class="row">
      <select bind:value={paletteKind}>
        <option>Color</option><option>Position</option><option>Beam</option><option>Intensity</option>
      </select>
      <button disabled={!programmer.length} onclick={() => savePalette(null)}>New palette</button>
      {#each show.palettes.filter((p) => p.kind === paletteKind) as p (p.id)}
        <button disabled={!programmer.length} onclick={() => savePalette(p.id)}>Update "{p.name}"</button>
      {/each}
    </div>
  </section>
</div>
{/if}

<style>
  .tabs { margin-bottom: 10px; }
  .desk { display: grid; grid-template-columns: minmax(240px, 1fr) minmax(320px, 1.4fr) minmax(260px, 1fr); gap: 12px; }
  @media (max-width: 1000px) { .desk { grid-template-columns: 1fr; } }
  .controls.disabled { opacity: 0.5; pointer-events: none; }
  .swatch { width: 34px; height: 34px; padding: 0; }
  .entry { justify-content: space-between; border-bottom: 1px solid var(--border); padding: 4px 0; }
  input[type='range'] { flex: 1; min-width: 120px; }
</style>
