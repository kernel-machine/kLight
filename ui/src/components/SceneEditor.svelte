<script lang="ts">
  // Scene contents: which groups/fixtures and which intentions.
  import { app, run } from '../lib/app.svelte';
  import ScenePreview from './ScenePreview.svelte';
  import { capabilityNames, colorToHex, describeTarget, describeValue, groupFixtures, hexToColor } from '../lib/show';
  import type { AttributeValue, FanMode, Function as Fn, Scene, SceneValue, Target, ValueSource } from '../lib/protocol';

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

  /** Same target and same kind of value: replaced instead of duplicated. */
  function sameSlot(a: SceneValue, b: SceneValue): boolean {
    if (JSON.stringify(a.target) !== JSON.stringify(b.target)) return false;
    if (a.value.type === 'palette' || b.value.type === 'palette') {
      return a.value.type === 'palette' && b.value.type === 'palette' && a.value.id === b.value.id;
    }
    return a.value.value.type === b.value.value.type;
  }

  // Edits are queued and each one is applied to the latest version of the
  // scene: quick successive clicks must not overwrite each other.
  let queue: Promise<unknown> = Promise.resolve();
  function update(change: (values: SceneValue[]) => SceneValue[]) {
    const id = f.id;
    queue = queue.then(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.kind.type !== 'scene') return;
      const values = change(current.kind.values);
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, values } } });
    });
  }

  function add() {
    const value = currentValue();
    if (!target || !value) return;
    const sv: SceneValue = { target, value };
    update((values) => [...values.filter((x) => !sameSlot(x, sv)), sv]);
  }

  function mergeProgrammer(replace: boolean) {
    const incoming: SceneValue[] = programmer.map((e) => ({ target: e.target, value: { type: 'explicit', value: e.value } }));
    update((values) => [...(replace ? [] : values.filter((x) => !incoming.some((y) => sameSlot(x, y)))), ...incoming]);
  }

  function removeValue(sv: SceneValue) {
    update((values) => values.filter((x) => JSON.stringify(x) !== JSON.stringify(sv)));
  }

  /** Loads an existing value into the form to edit it. */
  function load(sv: SceneValue) {
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

  /** Color a value sets (explicit, or from a color palette), as a CSS color. */
  function swatch(sv: SceneValue): string | null {
    if (sv.value.type === 'explicit') return sv.value.value.type === 'color' ? colorToHex(sv.value.value.color) : null;
    const id = sv.value.id;
    const first = show.palettes.find((p) => p.id === id)?.generic.find((v) => v.type === 'color');
    return first?.type === 'color' ? colorToHex(first.color) : null;
  }

  const describe = (sv: SceneValue) =>
    sv.value.type === 'explicit'
      ? describeValue(sv.value.value)
      : `Palette "${show.palettes.find((p) => sv.value.type === 'palette' && p.id === sv.value.id)?.name ?? '?'}"`;
</script>

<h3>Contents</h3>
<table>
  <tbody>
    {#each scene.values as sv, i (i)}
      <tr>
        <td>{describeTarget(show, sv.target)}</td>
        <td class="muted">
          {#if swatch(sv) !== null}<span class="swatch" style="background:{swatch(sv)}" title={swatch(sv)}></span>{/if}
          {describe(sv)}
        </td>
        <td class="actions">
          <button onclick={() => load(sv)}>Edit</button>
          <button onclick={() => removeValue(sv)}>×</button>
        </td>
      </tr>
    {:else}
      <tr><td class="muted">Empty scene: add a group or fixture below.</td></tr>
    {/each}
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
        <option value="intensity">Intensity</option>
        <option value="color">Color</option>
        <option value="position">Position</option>
        {#if target?.type === 'group'}<option value="fan">Fan (group order)</option>{/if}
        <option value="gobo">Gobo</option>
        <option value="prism">Prism</option>
        <option value="zoom">Zoom</option>
        <option value="strobe">Strobe</option>
        <option value="palette">Palette</option>
      </select>
    </label>
  </div>
  <div class="row">
    {#if kind === 'intensity'}
      <input type="range" min="0" max="100" bind:value={level} aria-label="Intensity" /> <span>{level}%</span>
    {:else if kind === 'color'}
      <input type="color" bind:value={color} aria-label="Color" />
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
    <button class="primary" disabled={!target || !currentValue()} onclick={add}>Add to scene</button>
  </div>
  <div class="row">
    <button disabled={!programmer.length} onclick={() => mergeProgrammer(false)}>Add programmer values</button>
    <button disabled={!programmer.length} onclick={() => mergeProgrammer(true)}>Replace with programmer</button>
  </div>
</div>

<ScenePreview functionId={f.id} />

<style>
  .add { margin-top: 10px; display: flex; flex-direction: column; gap: 8px; background: var(--panel-2); }
  .swatch { display: inline-block; width: 18px; height: 18px; border-radius: 4px; border: 1px solid var(--border); vertical-align: middle; margin-right: 6px; }
  .actions { text-align: right; white-space: nowrap; }
</style>
