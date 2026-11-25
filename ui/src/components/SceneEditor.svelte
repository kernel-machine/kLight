<script lang="ts">
  // Scene contents: which groups/fixtures and which intentions.
  import { app, notify, run } from '../lib/app.svelte';
  import SceneFaders from './SceneFaders.svelte';
  import { editQueue } from '../lib/queue';
  import { capabilityNames, colorToHex, describeTarget, describeValue, groupFixtures, hexToColor } from '../lib/show';
  import { addSceneValues, colorChoices, sceneValueColor, setSceneChannel, valueSupport } from '../lib/core';
  import type { AttributeValue, ChannelRef, FanMode, Function as Fn, Scene, SceneValue, Show, Target, ValueSource } from '../lib/protocol';

  let { f, scene }: { f: Fn; scene: Scene } = $props();
  const show = $derived(app.s.show!);
  const programmer = $derived(app.s.runtime?.state.programmer ?? []);

  type Kind = 'intensity' | 'color' | 'position' | 'fan' | 'gobo' | 'prism' | 'zoom' | 'strobe' | 'palette';
  let targetKey = $state('');
  let kind = $state<Kind>('intensity');
  let level = $state(100);
  let color = $state('#ff0000');
  let pan = $state(0);
  let tilt = $state(0);
  let spread = $state(60);
  let fanMode = $state<FanMode>('linear');
  let name = $state('');
  let zoom = $state(20);
  let hz = $state(5);
  let paletteId = $state<number | null>(null);

  const targets = $derived([
    ...show.groups.map((g) => ({ key: `g${g.id}`, label: `Group: ${g.name}`, target: { type: 'group', id: g.id } as Target })),
    ...show.fixtures.map((x) => ({ key: `f${x.id}`, label: x.name, target: { type: 'fixture', id: x.id } as Target })),
  ]);
  const target = $derived(targets.find((t) => t.key === targetKey)?.target ?? null);
  const targetFixtures = $derived.by(() => {
    if (!target) return [];
    if (target.type === 'group') {
      const g = show.groups.find((x) => x.id === target.id);
      return g ? groupFixtures(show, g) : [];
    }
    return show.fixtures.filter((x) => x.id === target.id);
  });
  // Only what the selected fixtures can do is offered (a palette always).
  const support = $derived(valueSupport(show, targetFixtures.map((x) => x.id)));
  const allowed = $derived<Kind[]>([
    ...(support.intensity ? (['intensity'] as const) : []),
    ...(support.color ? (['color'] as const) : []),
    ...(support.position ? (['position'] as const) : []),
    ...(support.position && target?.type === 'group' ? (['fan'] as const) : []),
    ...(support.gobo ? (['gobo'] as const) : []),
    ...(support.prism ? (['prism'] as const) : []),
    ...(support.zoom ? (['zoom'] as const) : []),
    ...(support.strobe ? (['strobe'] as const) : []),
    // Palettes are made in the Simple Desk: with none there is nothing to pick.
    ...(show.palettes.length ? (['palette'] as const) : []),
  ]);
  // Changing the target can leave the chosen attribute unavailable. Not
  // before a target is chosen: with none, only "palette" would be allowed and
  // the editor would open on it.
  $effect(() => {
    if (target && !allowed.includes(kind)) kind = allowed[0] ?? kind;
  });

  // Fixtures with a color wheel can only show its colors: those are offered
  // instead of a free picker (a fixture that mixes colors still gets one).
  const choices = $derived(colorChoices(show, targetFixtures.map((x) => x.id)));
  const onWheel = $derived(choices.wheel.some((w) => colorToHex(w.color) === color));
  const names = $derived(kind === 'gobo' || kind === 'prism' ? capabilityNames(show, targetFixtures, kind === 'gobo' ? 'Gobo' : 'Prism') : []);

  $effect(() => {
    if (!targetKey && targets[0]) targetKey = targets[0].key;
  });

  function currentValue(): ValueSource | null {
    const v = (value: AttributeValue): ValueSource => ({ type: 'explicit', value });
    switch (kind) {
      case 'intensity':
        return v({ type: 'intensity', level: level / 100 });
      case 'color':
        return v({ type: 'color', color: hexToColor(color) });
      case 'position':
        return v({ type: 'position', pan, tilt });
      case 'fan':
        return v({ type: 'position_fan', pan, tilt, pan_spread: spread, tilt_spread: 0, mode: fanMode });
      case 'gobo':
        return name ? v({ type: 'gobo', name }) : null;
      case 'prism':
        return name ? v({ type: 'prism', name }) : null;
      case 'zoom':
        return v({ type: 'zoom', degrees: zoom });
      case 'strobe':
        return v({ type: 'strobe', strobe: { type: 'Hz', hz } });
      case 'palette':
        return paletteId === null ? null : { type: 'palette', id: paletteId };
    }
  }

  // Edits are queued and each one is applied to the latest version of the
  // scene: quick successive clicks must not overwrite each other.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function update(change: (values: SceneValue[], show: Show) => SceneValue[]) {
    const id = f.id;
    return enqueue(() => {
      const latest = app.s.show;
      const current = latest?.functions.find((x) => x.id === id);
      if (!latest || !current || current.kind.type !== 'scene') return;
      const values = change(current.kind.values, latest);
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, values } } });
    });
  }

  function add() {
    const value = currentValue();
    if (!target || !value) return;
    const sv: SceneValue = { target, value };
    update((values, latest) => addSceneValues(latest, values, [sv]));
  }

  function mergeProgrammer(replace: boolean) {
    const incoming: SceneValue[] = programmer.map((e) => ({ target: e.target, value: { type: 'explicit', value: e.value } }));
    update((values, latest) => addSceneValues(latest, replace ? [] : values, incoming));
  }

  const setChannel = (fixture: number, channel: ChannelRef, value: number | null) =>
    update((values) => setSceneChannel(values, fixture, channel, value));

  /** Channels set by hand: listed once per fixture, edited on the faders. */
  const manual = $derived.by(() => {
    const counts = new Map<number, number>();
    for (const sv of scene.values) {
      if (sv.value.type === 'channel' && sv.target.type === 'fixture') counts.set(sv.target.id, (counts.get(sv.target.id) ?? 0) + 1);
    }
    return [...counts];
  });

  function releaseFixture(id: number) {
    update((values) => values.filter((x) => !(x.value.type === 'channel' && x.target.type === 'fixture' && x.target.id === id)));
  }

  function removeValue(sv: SceneValue) {
    update((values) => values.filter((x) => JSON.stringify(x) !== JSON.stringify(sv)));
  }

  /** Loads an existing value into the form to edit it. */
  function load(sv: SceneValue) {
    if (sv.value.type === 'channel') return;
    const t = targets.find((x) => JSON.stringify(x.target) === JSON.stringify(sv.target));
    if (t) targetKey = t.key;
    if (sv.value.type === 'palette') {
      kind = 'palette';
      paletteId = sv.value.id;
      return;
    }
    const v = sv.value.value;
    switch (v.type) {
      case 'intensity': kind = 'intensity'; level = Math.round(v.level * 100); break;
      case 'color': kind = 'color'; color = colorToHex(v.color); break;
      case 'position': kind = 'position'; pan = v.pan; tilt = v.tilt; break;
      case 'position_fan': kind = 'fan'; pan = v.pan; tilt = v.tilt; spread = v.pan_spread; fanMode = v.mode; break;
      case 'gobo': kind = 'gobo'; name = v.name; break;
      case 'prism': kind = 'prism'; name = v.name; break;
      case 'zoom': kind = 'zoom'; zoom = v.degrees; break;
      case 'strobe': kind = 'strobe'; if (v.strobe.type === 'Hz') hz = v.strobe.hz; break;
      default: break;
    }
  }

  /** Color the i-th value sets (explicit, or from a color palette), as a CSS color; from the core. */
  function swatch(i: number): string | null {
    const c = sceneValueColor(show, f.id, i);
    return c ? colorToHex(c) : null;
  }

  function describe(sv: SceneValue): string {
    const v = sv.value;
    switch (v.type) {
      case 'explicit':
        return describeValue(v.value);
      case 'palette':
        return `Palette "${show.palettes.find((p) => p.id === v.id)?.name ?? '?'}"`;
      case 'channel':
        return `Channel ${v.value}`;
    }
  }
</script>

<h3>Contents</h3>
<table>
  <tbody>
    {#each scene.values as sv, i (i)}
      {#if sv.value.type !== 'channel'}
        <tr>
          <td>{describeTarget(show, sv.target)}</td>
          <!-- No whitespace around the text: the cell reads exactly as the value. -->
          <td class="muted">{#if swatch(i) !== null}<span class="swatch" style="background:{swatch(i)}" title={swatch(i)}></span>{/if}{describe(sv)}</td>
          <td class="actions">
            <button onclick={() => load(sv)}>Edit</button>
            <button onclick={() => removeValue(sv)}>×</button>
          </td>
        </tr>
      {/if}
    {/each}
    {#each manual as [id, count] (id)}
      <tr>
        <td>{describeTarget(show, { type: 'fixture', id })}</td>
        <td class="muted">{count} channel{count === 1 ? '' : 's'} set by hand</td>
        <td class="actions"><button title="Release these channels" onclick={() => releaseFixture(id)}>×</button></td>
      </tr>
    {/each}
    {#if !scene.values.length}
      <tr><td class="muted">Empty scene: add a group or fixture below.</td></tr>
    {/if}
  </tbody>
</table>

<div class="add panel">
  <div class="row">
    <label class="row">Group / fixture
      <select bind:value={targetKey} aria-label="Scene target">
        {#each targets as t (t.key)}<option value={t.key}>{t.label}</option>{/each}
      </select>
    </label>
    <label class="row">Attribute
      <select bind:value={kind} aria-label="Scene attribute">
        {#if allowed.includes('intensity')}<option value="intensity">Intensity</option>{/if}
        {#if allowed.includes('color')}<option value="color">Color</option>{/if}
        {#if allowed.includes('position')}<option value="position">Position</option>{/if}
        {#if allowed.includes('fan')}<option value="fan">Fan (group order)</option>{/if}
        {#if allowed.includes('gobo')}<option value="gobo">Gobo</option>{/if}
        {#if allowed.includes('prism')}<option value="prism">Prism</option>{/if}
        {#if allowed.includes('zoom')}<option value="zoom">Zoom</option>{/if}
        {#if allowed.includes('strobe')}<option value="strobe">Strobe</option>{/if}
        {#if allowed.includes('palette')}<option value="palette">Palette</option>{/if}
      </select>
    </label>
  </div>
  <div class="row">
    {#if !allowed.length}
      <span class="muted">Nothing to set: this target has no fixtures, and there are no palettes yet (create them in the Simple Desk).</span>
    {:else if kind === 'intensity'}
      <input type="range" min="0" max="100" bind:value={level} aria-label="Intensity" /> <span>{level}%</span>
    {:else if kind === 'color'}
      {#if choices.wheel.length}
        <div class="wheel" role="radiogroup" aria-label="Color wheel colors">
          {#each choices.wheel as w (colorToHex(w.color))}
            {@const hex = colorToHex(w.color)}
            <button class="chip" class:on={hex === color} role="radio" aria-checked={hex === color} aria-label={w.name} title={w.name} style="--c:{hex}" onclick={() => (color = hex)}></button>
          {/each}
        </div>
      {/if}
      {#if choices.free}<input type="color" bind:value={color} aria-label="Color" />{/if}
      {#if !choices.free && !onWheel}<span class="muted">Choose one of the wheel colors.</span>{/if}
    {:else if kind === 'position'}
      <label>Pan <input type="number" min="-270" max="270" step="0.5" bind:value={pan} style="width:80px" />°</label>
      <label>Tilt <input type="number" min="-135" max="135" step="0.5" bind:value={tilt} style="width:80px" />°</label>
    {:else if kind === 'fan'}
      <label>Pan <input type="number" min="-270" max="270" step="0.5" bind:value={pan} style="width:72px" />°</label>
      <label>Tilt <input type="number" min="-135" max="135" step="0.5" bind:value={tilt} style="width:72px" />°</label>
      <label>Spread <input type="number" min="-540" max="540" step="1" bind:value={spread} style="width:72px" />°</label>
      <select bind:value={fanMode}><option value="linear">linear</option><option value="from_center">from center</option></select>
    {:else if kind === 'gobo' || kind === 'prism'}
      <select bind:value={name}>
        <option value="">— choose —</option>
        {#each names as n (n)}<option value={n}>{n}</option>{/each}
      </select>
      {#if !names.length}<span class="muted">The selected fixtures have no {kind}.</span>{/if}
    {:else if kind === 'zoom'}
      <label>Zoom <input type="number" min="1" max="90" bind:value={zoom} style="width:70px" />°</label>
    {:else if kind === 'strobe'}
      <label>Strobe <input type="number" min="0.5" max="30" step="0.5" bind:value={hz} style="width:70px" /> Hz</label>
    {:else}
      <select bind:value={paletteId}>
        <option value={null}>— choose —</option>
        {#each show.palettes as p (p.id)}<option value={p.id}>{p.name} ({p.kind})</option>{/each}
      </select>
    {/if}
    <button class="primary" disabled={!target || !currentValue() || (kind === 'color' && !choices.free && !onWheel)} onclick={add}>Add to scene</button>
  </div>
  <div class="row">
    <button disabled={!programmer.length} onclick={() => mergeProgrammer(false)}>Add programmer values</button>
    <button disabled={!programmer.length} onclick={() => mergeProgrammer(true)}>Replace with programmer</button>
  </div>
</div>

<SceneFaders functionId={f.id} onset={setChannel} />

<style>
  .add { margin-top: 10px; display: flex; flex-direction: column; gap: 8px; background: var(--panel-2); }
  .wheel { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { width: 26px; height: 26px; padding: 0; border-radius: 6px; background: var(--c); border: 2px solid var(--border); cursor: pointer; }
  .chip:hover { border-color: var(--muted); }
  .chip.on { border-color: var(--text); box-shadow: 0 0 0 2px var(--accent); }
  .swatch { display: inline-block; width: 18px; height: 18px; border-radius: 4px; border: 1px solid var(--border); vertical-align: middle; margin-right: 6px; }
  .actions { text-align: right; white-space: nowrap; }
</style>
