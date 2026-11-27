<script lang="ts">
  // Scene contents: its lights (groups and fixtures), the values given to all
  // of them or to some, and their channels. Laid out in the order of the work:
  // a header (name, test, rarely used actions in a menu), the lights and the
  // panel that sets a value side by side and level, the values or the
  // channels (one at a time), and the times folded at the bottom. A control
  // moved in the panel goes straight into the scene (Undo takes it back).
  import { untrack } from 'svelte';
  import { app, client, notify, pref, run, setPref } from '../lib/app.svelte';
  import { livePreview } from '../lib/livePreview';
  import SceneFaders from './SceneFaders.svelte';
  import ApplyTo from './ApplyTo.svelte';
  import LightPickerDialog from './LightPickerDialog.svelte';
  import SceneTiming from './SceneTiming.svelte';
  import XYPad from './XYPad.svelte';
  import { editQueue } from '../lib/queue';
  import { capabilityNames, colorToHex, describeTarget, describeValue, hexToColor } from '../lib/show';
  import { addSceneMembers, addSceneValues, colorChoices, coveredValues, removeCoveredValues, removeSceneMember, sceneLightLooks, sceneLights, sceneValueColor, setSceneChannel, valueSupport } from '../lib/core';
  import type { AttributeValue, ChannelRef, CoveredValue, FanMode, Function as Fn, Scene, SceneValue, Show, Target, ValueSource } from '../lib/protocol';

  let {
    f,
    scene,
    running = false,
    onrun,
    onduplicate,
    ondelete,
  }: {
    f: Fn;
    scene: Scene;
    /** The scene plays, with its own times. */
    running?: boolean;
    /** Starts (true) or stops the scene. */
    onrun: (start: boolean) => void;
    onduplicate: () => void;
    ondelete: () => void;
  } = $props();
  const show = $derived(app.s.show!);
  const programmer = $derived(app.s.runtime?.state.programmer ?? []);

  type Kind = 'intensity' | 'color' | 'position' | 'fan' | 'gobo' | 'prism' | 'zoom' | 'strobe' | 'palette';
  /** What new values go to: one group, some fixtures, or (neither) all lights. */
  let toGroup = $state<number | null>(null);
  let toFixtures = $state<number[]>([]);
  let adding = $state(false);
  let kind = $state<Kind>('intensity');
  let level = $state(100);
  let color = $state('#ff0000');
  let pan = $state(0);
  let tilt = $state(0);
  let spread = $state(60);
  let tiltSpread = $state(0);
  let fanMode = $state<FanMode>('linear');
  let name = $state('');
  let zoom = $state(20);
  let hz = $state(5);
  let panDmx = $state(128);
  let tiltDmx = $state(128);
  let strobeDmx = $state(128);
  /** Chosen where some lights have units and others only DMX values. */
  let preferDmx = $state({ position: false, strobe: false });
  let paletteId = $state<number | null>(null);

  const lights = $derived(sceneLights(show, f.id));
  const memberGroups = $derived(
    lights.members.flatMap((m) => (m.target.type === 'group' ? show.groups.filter((g) => g.id === (m.target as { id: number }).id) : [])),
  );
  const fixtureName = (id: number) => show.fixtures.find((x) => x.id === id)?.name ?? `Fixture ${id}`;
  // A light that left the scene is no longer chosen.
  const group = $derived(memberGroups.some((g) => g.id === toGroup) ? toGroup : null);
  const chosen = $derived(group === null ? toFixtures.filter((id) => lights.fixtures.includes(id)) : []);
  const all = $derived(group === null && !chosen.length);
  const targets = $derived<Target[]>(
    !lights.members.length
      ? []
      : group !== null
        ? [{ type: 'group', id: group }]
        : chosen.length
          ? chosen.map((id) => ({ type: 'fixture', id }))
          : [{ type: 'scene' }],
  );
  const targetFixtures = $derived.by(() => {
    const ids =
      group !== null
        ? (lights.members.find((m) => m.target.type === 'group' && m.target.id === group)?.fixtures ?? [])
        : chosen.length
          ? chosen
          : lights.fixtures;
    return show.fixtures.filter((x) => ids.includes(x.id));
  });
  // Another scene starts from all of its lights. (`f` is replaced at every
  // edit: only a change of its id counts.)
  const sceneId = $derived(f.id);
  $effect(() => {
    void sceneId;
    toAll();
    adding = false;
    dragged = null;
    lit = [];
    menuOpen = false;
  });

  function toAll() {
    toGroup = null;
    toFixtures = [];
  }
  function chooseGroup(id: number) {
    toGroup = group === id ? null : id;
    toFixtures = [];
  }
  function chooseFixtures(ids: number[]) {
    toFixtures = ids;
    toGroup = null;
  }
  // Only what the selected fixtures can do is offered (a palette always).
  const support = $derived(valueSupport(show, targetFixtures.map((x) => x.id)));
  const allowed = $derived<Kind[]>([
    ...(support.intensity ? (['intensity'] as const) : []),
    ...(support.color ? (['color'] as const) : []),
    ...(support.position ? (['position'] as const) : []),
    ...(support.position_degrees && group !== null ? (['fan'] as const) : []),
    ...(support.gobo ? (['gobo'] as const) : []),
    ...(support.prism ? (['prism'] as const) : []),
    ...(support.zoom ? (['zoom'] as const) : []),
    ...(support.strobe ? (['strobe'] as const) : []),
    // Palettes are made in the Simple Desk: with none there is nothing to pick.
    ...(show.palettes.length ? (['palette'] as const) : []),
  ]);
  // Pan, tilt and strobe are plain DMX values where the definitions give no
  // degrees or hertz (the core says which lights do).
  const mixed = $derived({
    position: support.position_degrees && support.position_dmx,
    strobe: support.strobe_hz && support.strobe_dmx,
  });
  const positionDmx = $derived(mixed.position ? preferDmx.position : support.position_dmx);
  const strobeAsDmx = $derived(mixed.strobe ? preferDmx.strobe : support.strobe_dmx);
  const byte = (n: number) => Math.max(0, Math.min(255, Math.round(Number(n) || 0)));

  // Changing the lights can leave the chosen attribute unavailable. Not while
  // there are none: only "palette" would be allowed and the editor would open
  // on it.
  $effect(() => {
    if (targets.length && !allowed.includes(kind)) kind = allowed[0] ?? kind;
  });

  // Fixtures with a color wheel can only show its colors: those are offered
  // instead of a free picker (a fixture that mixes colors still gets one).
  const choices = $derived(colorChoices(show, targetFixtures.map((x) => x.id)));
  const onWheel = $derived(choices.wheel.some((w) => colorToHex(w.color) === color));
  const names = $derived(kind === 'gobo' || kind === 'prism' ? capabilityNames(show, targetFixtures, kind === 'gobo' ? 'Gobo' : 'Prism') : []);

  /** The values the panel would add (a DMX position is two); none while incomplete. */
  function currentValues(): ValueSource[] {
    const v = (value: AttributeValue): ValueSource => ({ type: 'explicit', value });
    const raw = (attribute: 'Pan' | 'Tilt' | 'Shutter', n: number) => v({ type: 'raw', attribute, value: byte(n) });
    switch (kind) {
      case 'intensity':
        return [v({ type: 'intensity', level: level / 100 })];
      case 'color':
        return [v({ type: 'color', color: hexToColor(color) })];
      case 'position':
        return positionDmx ? [raw('Pan', panDmx), raw('Tilt', tiltDmx)] : [v({ type: 'position', pan, tilt })];
      case 'fan':
        return [v({ type: 'position_fan', pan, tilt, pan_spread: spread, tilt_spread: tiltSpread, mode: fanMode })];
      case 'gobo':
        return name ? [v({ type: 'gobo', name })] : [];
      case 'prism':
        return name ? [v({ type: 'prism', name })] : [];
      case 'zoom':
        return [v({ type: 'zoom', degrees: zoom })];
      case 'strobe':
        return strobeAsDmx ? [raw('Shutter', strobeDmx)] : [v({ type: 'strobe', strobe: { type: 'Hz', hz } })];
      case 'palette':
        return paletteId === null ? [] : [{ type: 'palette', id: paletteId }];
    }
  }

  // Edits are queued and each one is applied to the latest version of the
  // scene: quick successive clicks must not overwrite each other.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function update(change: (scene: Scene, show: Show) => Scene) {
    const id = f.id;
    return enqueue(() => {
      const latest = app.s.show;
      const current = latest?.functions.find((x) => x.id === id);
      if (!latest || !current || current.kind.type !== 'scene') return;
      const next = change(current.kind, latest);
      return run({ type: 'set_function', function: { ...current, kind: { ...next, type: 'scene' } } });
    });
  }
  /**
   * Renamed in the same queue as the other edits, on the latest scene: a copy
   * taken before values just set came back would drop them.
   */
  function rename(name: string) {
    const id = f.id;
    enqueue(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.name === name) return;
      return run({ type: 'set_function', function: { ...current, name } });
    });
  }
  const updateValues = (change: (values: SceneValue[]) => SceneValue[]) =>
    update((s) => ({ ...s, values: change(s.values) }));

  function add() {
    const values = currentValues();
    if (!targets.length || !values.length) return;
    const incoming: SceneValue[] = targets.flatMap((target) => values.map((value) => ({ target, value })));
    update((s, latest) => addSceneValues(latest, s, incoming));
  }

  /**
   * A control was let go (or a choice made): its value goes into the scene,
   * one undo step. Nothing while the value is incomplete, or is a color the
   * wheel cannot show.
   */
  function commit() {
    touched();
    if (kind === 'color' && !choices.free && !onWheel) return;
    add();
  }

  function addLights(more: Target[]) {
    adding = false;
    update((s, latest) => addSceneMembers(latest, s, more));
  }

  function mergeProgrammer(replace: boolean) {
    const incoming: SceneValue[] = programmer.map((e) => ({ target: e.target, value: { type: 'explicit', value: e.value } }));
    update((s, latest) => addSceneValues(latest, replace ? { ...s, values: [] } : s, incoming));
  }

  const setChannel = (fixture: number, channel: ChannelRef, value: number | null) =>
    updateValues((values) => setSceneChannel(values, fixture, channel, value));

  /** Channels set by hand: listed once per fixture, edited on the faders. */
  const manual = $derived.by(() => {
    const counts = new Map<number, number>();
    for (const sv of scene.values) {
      if (sv.value.type === 'channel' && sv.target.type === 'fixture') counts.set(sv.target.id, (counts.get(sv.target.id) ?? 0) + 1);
    }
    return [...counts];
  });

  function releaseFixture(id: number) {
    updateValues((values) => values.filter((x) => !(x.value.type === 'channel' && x.target.type === 'fixture' && x.target.id === id)));
  }

  function removeValue(sv: SceneValue) {
    updateValues((values) => values.filter((x) => JSON.stringify(x) !== JSON.stringify(sv)));
  }

  /** The preset panel, above the values: brought into view to edit one. */
  let panel = $state<HTMLElement>();

  // ---- the header's menu of rarely used actions.
  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  /** A menu item: closes the menu, then acts. */
  const act = (fn: () => void) => () => {
    menuOpen = false;
    fn();
  };
  function outside(e: MouseEvent) {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }

  /** The values as a list, or every channel on faders: one at a time. */
  let view = $state<'values' | 'channels'>(pref<string>('scene.view', 'values') === 'channels' ? 'channels' : 'values');
  $effect(() => setPref('scene.view', view));

  const ATTRIBUTES: Array<[Kind, string]> = [
    ['intensity', 'Intensity'],
    ['color', 'Color'],
    ['position', 'Position'],
    ['fan', 'Fan (group order)'],
    ['gobo', 'Gobo'],
    ['prism', 'Prism'],
    ['zoom', 'Zoom'],
    ['strobe', 'Strobe'],
    ['palette', 'Palette'],
  ];
  // ---- live preview: the lights show the scene, with what is being tried
  // (a control moved in the panel, a channel fader being dragged).
  let wantsPreview = $state(pref('scene.preview', false));
  $effect(() => setPref('scene.preview', wantsPreview));
  const canPreview = $derived(
    app.s.connection === 'connected' && app.s.permission === 'full' && app.s.runtime?.state.mode === 'Design',
  );
  /** What the panel is about: the controls count once moved for it. */
  const aim = $derived(JSON.stringify([sceneId, kind, targets]));
  let movedFor = $state('');
  /**
   * Starting the scene pauses the preview, which would hide its fades; moving
   * a control or ticking the box again goes on with it.
   */
  let paused = $state(false);
  $effect(() => {
    if (running) paused = true;
  });
  /** A control was moved for what the panel is about. */
  function touched() {
    movedFor = aim;
    paused = false;
  }
  /** Fixtures lit at full, in white where they mix colors: to find them on stage. */
  let lit = $state<number[]>([]);
  const toggleLit = (id: number) => (lit = lit.includes(id) ? lit.filter((x) => x !== id) : [...lit, id]);
  const litValues = $derived<SceneValue[]>(
    !canPreview
      ? []
      : lit
          .filter((id) => lights.fixtures.includes(id))
          .flatMap((id): SceneValue[] => {
            const target: Target = { type: 'fixture', id };
            return [
              { target, value: { type: 'explicit', value: { type: 'intensity', level: 1 } } },
              ...(valueSupport(show, [id]).color ? [{ target, value: { type: 'explicit', value: { type: 'color', color: { r: 1, g: 1, b: 1 } } } } as SceneValue] : []),
            ];
          }),
  );
  /** Color and intensity the scene gives each fixture, shown on the lights. */
  const looks = $derived(sceneLightLooks(show, f.id));
  // Turning to something else starts afresh, also when coming back to it.
  $effect.pre(() => {
    void aim;
    movedFor = '';
  });
  let dragged = $state<{ fixture: number; channel: ChannelRef; value: number } | null>(null);
  const trial = $derived.by(() => {
    if (!wantsPreview || !canPreview || paused) return null;
    let s: Scene = scene;
    const values = movedFor === aim ? currentValues() : [];
    if (values.length && targets.length) {
      s = addSceneValues(show, s, targets.flatMap((target) => values.map((value) => ({ target, value }))));
    }
    return dragged ? { ...s, values: setSceneChannel(s.values, dragged.fixture, dragged.channel, dragged.value) } : s;
  });
  let failed = false;
  const preview = livePreview((command) => {
    client.command(command).catch((e: Error) => {
      // Told once: it is sent over and over.
      if (!failed) notify(`Preview: ${e.message}`, 'error');
      failed = true;
    });
  });
  $effect(() => {
    // The highlight comes last: it wins over the scene on the chosen lights.
    if (trial || litValues.length) preview.show([...(trial?.values ?? []), ...litValues], trial?.members ?? scene.members);
    else preview.stop();
  });
  // Leaving the editor gives the lights back.
  $effect(() => () => preview.stop());

  // ---- values that later ones override: the last value wins on every channel.
  const covers = $derived(new Map<number, CoveredValue>(coveredValues(show, scene).map((c) => [c.index, c])));
  const fullyCovered = $derived([...covers.values()].filter((c) => c.full).length);
  /** Who overrides a value, in words (each target once). */
  function coveredBy(c: CoveredValue): string {
    const names = [...new Set(c.by.flatMap((j) => { const v = scene.values[j]; return v ? [describeTarget(show, v.target)] : []; }))];
    return c.full ? `No effect: overridden by ${names.join(', ')}` : `Overridden on some lights by ${names.join(', ')}`;
  }

  /** Who the next value goes to, in words. */
  const targetName = $derived(
    group !== null
      ? `Group ${memberGroups.find((g) => g.id === group)?.name}`
      : chosen.length === 1
        ? fixtureName(chosen[0]!)
        : chosen.length
          ? `${chosen.length} chosen fixtures`
          : 'All lights',
  );

  /** The DMX value a scene gives an attribute of a target, if it does. */
  function rawOf(target: Target, attribute: string): number | undefined {
    for (const sv of scene.values) {
      if (JSON.stringify(sv.target) !== JSON.stringify(target) || sv.value.type !== 'explicit') continue;
      if (sv.value.value.type === 'raw' && sv.value.value.attribute === attribute) return sv.value.value.value;
    }
    return undefined;
  }

  /** The attribute of the panel a value is for (none for channels set by hand). */
  function kindOf(sv: SceneValue): Kind | null {
    if (sv.value.type === 'palette') return 'palette';
    if (sv.value.type !== 'explicit') return null;
    const v = sv.value.value;
    switch (v.type) {
      case 'position_fan': return 'fan';
      case 'raw': return v.attribute === 'Shutter' ? 'strobe' : v.attribute === 'Pan' || v.attribute === 'Tilt' ? 'position' : null;
      case 'intensity': case 'color': case 'position': case 'gobo': case 'prism': case 'zoom': case 'strobe': return v.type;
      default: return null;
    }
  }

  /** Chooses the lights and the attribute of a value, to change it. */
  function load(sv: SceneValue) {
    const k = kindOf(sv);
    if (!k) return;
    panel?.scrollIntoView({ block: 'nearest' });
    const t = sv.target;
    toAll();
    if (t.type === 'group') toGroup = t.id;
    else if (t.type === 'fixture' || t.type === 'head') toFixtures = [t.id];
    kind = k;
    show_(sv);
  }

  // Choosing lights and an attribute shows the value the scene gives them
  // (the first chosen light's), so a change starts from it. Only then: the
  // show coming back from the engine must not overwrite what is being typed.
  $effect(() => {
    void aim;
    untrack(() => {
      const first = targets[0];
      if (!first) return;
      const key = JSON.stringify(first);
      const sv = [...scene.values].reverse().find((x) => JSON.stringify(x.target) === key && kindOf(x) === kind);
      if (sv) show_(sv);
    });
  });

  /** Puts a value in the controls. */
  function show_(sv: SceneValue) {
    if (sv.value.type === 'palette') {
      paletteId = sv.value.id;
      return;
    }
    if (sv.value.type !== 'explicit') return;
    const v = sv.value.value;
    switch (v.type) {
      case 'intensity': level = Math.round(v.level * 100); break;
      case 'color': color = colorToHex(v.color); break;
      case 'position': preferDmx.position = false; pan = v.pan; tilt = v.tilt; break;
      case 'position_fan': pan = v.pan; tilt = v.tilt; spread = v.pan_spread; tiltSpread = v.tilt_spread; fanMode = v.mode; break;
      case 'gobo': name = v.name; break;
      case 'prism': name = v.name; break;
      case 'zoom': zoom = v.degrees; break;
      case 'strobe': preferDmx.strobe = false; if (v.strobe.type === 'Hz') hz = v.strobe.hz; break;
      case 'raw':
        if (v.attribute === 'Shutter') {
          preferDmx.strobe = true;
          strobeDmx = v.value;
        } else if (v.attribute === 'Pan' || v.attribute === 'Tilt') {
          preferDmx.position = true;
          // Pan and tilt are edited together: the other one comes along.
          panDmx = rawOf(sv.target, 'Pan') ?? panDmx;
          tiltDmx = rawOf(sv.target, 'Tilt') ?? tiltDmx;
        }
        break;
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

<svelte:window onclick={outside} />

<!-- Header: name; testing the scene (its own fades, or live on the lights);
     the rarely used actions in a menu. -->
<div class="head">
  <input class="name" value={f.name} aria-label="Scene name" onchange={(e) => rename(e.currentTarget.value)} />
  <span class="badge">Scene</span>
  <span class="spacer"></span>
  <div class="test" role="group" aria-label="Test">
    {#if running}
      <button class="primary" onclick={() => onrun(false)}>Stop</button>
    {:else}
      <button title="Runs the scene with its times, to check its fades (the live preview pauses)" onclick={() => onrun(true)}>Start</button>
    {/if}
    <label class="live" title={canPreview ? 'The lights show this scene and follow the controls as they move' : 'Only in Design mode, with full access'}>
      <input type="checkbox" bind:checked={wantsPreview} disabled={!canPreview} onchange={() => (paused = false)} />
      Live preview on the lights
    </label>
  </div>
  <div class="menu-wrap" bind:this={menuWrap}>
    <button class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>⋯</button>
    {#if menuOpen}
      <div class="menu" role="menu" tabindex="-1" onkeydown={(e) => { if (e.key === 'Escape') menuOpen = false; }}>
        <button role="menuitem" onclick={act(onduplicate)}>Duplicate</button>
        <button role="menuitem" disabled={!programmer.length} onclick={act(() => mergeProgrammer(false))}>Add programmer values</button>
        <button role="menuitem" disabled={!programmer.length} onclick={act(() => mergeProgrammer(true))}>Replace with programmer</button>
        <button role="menuitem" disabled={!fullyCovered} title="Removes the values that later ones override on every channel: the lights do not change" onclick={act(() => update((s, latest) => removeCoveredValues(latest, s)))}>Remove {fullyCovered || ''} value{fullyCovered === 1 ? '' : 's'} with no effect</button>
        <hr />
        <button role="menuitem" class="danger-text" onclick={act(ondelete)}>Delete scene</button>
      </div>
    {/if}
  </div>
</div>
{#if wantsPreview && canPreview && paused}
  <p class="muted small paused">Live preview paused while the scene was started, so its fades show. Move a control to go on.</p>
{/if}

{#snippet addLightsRow()}
  <div class="row add-lights">
    <button class="primary" onclick={() => (adding = true)}>Add lights…</button>
    {#if lights.members.length}
      <label class="row small" title="When off, lights added later do not take the values given to all lights">
        <input type="checkbox" checked={scene.new_members_follow} onchange={(e) => { const on = e.currentTarget.checked; update((s) => ({ ...s, new_members_follow: on })); }} />
        Lights added later take the values of all lights
      </label>
    {/if}
  </div>
{/snippet}

<!-- Units or plain DMX values, where the chosen lights allow both. -->
{#snippet unit(what: 'position' | 'strobe', label: string, missing: string)}
  <div class="unit" role="radiogroup" aria-label="Unit">
    <button role="radio" aria-checked={!preferDmx[what]} class:on={!preferDmx[what]} onclick={() => (preferDmx[what] = false)}>{label}</button>
    <button role="radio" aria-checked={preferDmx[what]} class:on={preferDmx[what]} onclick={() => (preferDmx[what] = true)}>DMX 0–255</button>
  </div>
  <span class="muted small">Some of these lights have no {missing}. DMX writes the same value on all of them.</span>
{/snippet}

<!-- A scene starts from its lights: the rest appears once it has some. -->
{#if !lights.members.length}
  <section class="card empty" aria-label="Lights">
    <h3>Lights</h3>
    <p class="muted">No lights yet: add the groups and fixtures this scene drives.</p>
    {@render addLightsRow()}
    {#if programmer.length}
      <!-- The programmer brings its own lights. -->
      <div class="row">
        <button onclick={() => mergeProgrammer(false)}>Add programmer values</button>
      </div>
    {/if}
  </section>
{:else}
<div class="compose">
  <div class="steps">
    <section class="card pick" aria-label="Choose lights">
      <h3><span class="step">1</span> Lights <small class="muted">· {lights.fixtures.length}</small></h3>
      {@render addLightsRow()}
      {#key sceneId}
        <ApplyTo
          {lights}
          groups={memberGroups}
          {group}
          {chosen}
          {looks}
          {lit}
          canLight={canPreview}
          onlight={toggleLit}
          onall={toAll}
          ongroup={chooseGroup}
          onfixtures={chooseFixtures}
          onremove={(light) => update((s, latest) => removeSceneMember(latest, s, light))}
        />
      {/key}
    </section>

    <section class="card preset" aria-label="Set a value" bind:this={panel}>
      <h3><span class="step">2</span> Set <span class="target">on <b>{targetName}</b> {#if !chosen.length}<span class="muted">· {targetFixtures.length} fixture{targetFixtures.length === 1 ? '' : 's'}</span>{/if}</span></h3>
      <div class="attributes" role="radiogroup" aria-label="Scene attribute">
        {#each ATTRIBUTES.filter(([k]) => allowed.includes(k)) as [k, label] (k)}
          <button role="radio" aria-checked={kind === k} class:on={kind === k} onclick={() => (kind = k)}>{label}</button>
        {/each}
      </div>
      <!-- Moving a control previews it; letting it go puts it in the scene
           (not the pad's Fine box, which changes no value). -->
      <div class="control" role="presentation" oninput={touched} onchange={(e) => { if ((e.target as HTMLInputElement).type !== 'checkbox') commit(); }}>
        {#if !allowed.length}
          <span class="muted">Nothing to set: these lights have no fixtures, and there are no palettes yet (create them in the Simple Desk).</span>
        {:else if kind === 'intensity'}
          <span class="level">{level}%</span>
          <input type="range" min="0" max="100" bind:value={level} aria-label="Intensity" />
          <div class="quick" role="group" aria-label="Quick levels">
            {#each [0, 50, 100] as q (q)}
              <button onclick={() => { level = q; commit(); }}>{q}%</button>
            {/each}
          </div>
        {:else if kind === 'color'}
          {#if choices.wheel.length}
            <div class="wheel" role="radiogroup" aria-label="Color wheel colors">
              {#each choices.wheel as w (colorToHex(w.color))}
                {@const hex = colorToHex(w.color)}
                <button class="chip" class:on={hex === color} role="radio" aria-checked={hex === color} aria-label={w.name} title={w.name} style="--c:{hex}" onclick={() => { color = hex; commit(); }}></button>
              {/each}
            </div>
          {/if}
          {#if choices.free}<input type="color" bind:value={color} aria-label="Color" />{/if}
          {#if !choices.free && !onWheel}<span class="muted">Choose one of the wheel colors.</span>{/if}
        {:else if kind === 'position'}
          {#if mixed.position}{@render unit('position', 'Degrees', 'range in degrees')}{/if}
          {#if positionDmx}
            <XYPad pan={panDmx} tilt={tiltDmx} panRange={[0, 255]} tiltRange={[0, 255]} unit="" onchange={(p, t) => { panDmx = Math.round(p); tiltDmx = Math.round(t); touched(); }} oncommit={commit} />
            <label>Pan <input type="range" min="0" max="255" bind:value={panDmx} aria-label="Pan DMX" /><input class="byte" type="number" min="0" max="255" bind:value={panDmx} aria-label="Pan DMX value" /></label>
            <label>Tilt <input type="range" min="0" max="255" bind:value={tiltDmx} aria-label="Tilt DMX" /><input class="byte" type="number" min="0" max="255" bind:value={tiltDmx} aria-label="Tilt DMX value" /></label>
          {:else}
            <XYPad {pan} {tilt} panRange={[-270, 270]} tiltRange={[-135, 135]} onchange={(p, t) => { pan = p; tilt = t; touched(); }} oncommit={commit} />
            <div class="pair">
              <label>Pan <input type="number" min="-270" max="270" step="0.5" bind:value={pan} aria-label="Pan degrees" />°</label>
              <label>Tilt <input type="number" min="-135" max="135" step="0.5" bind:value={tilt} aria-label="Tilt degrees" />°</label>
            </div>
          {/if}
        {:else if kind === 'fan'}
          <XYPad {pan} {tilt} panRange={[-270, 270]} tiltRange={[-135, 135]} onchange={(p, t) => { pan = p; tilt = t; touched(); }} oncommit={commit} />
          <div class="pair">
            <label>Pan <input type="number" min="-270" max="270" step="0.5" bind:value={pan} />°</label>
            <label>Tilt <input type="number" min="-135" max="135" step="0.5" bind:value={tilt} />°</label>
          </div>
          <div class="pair">
            <label>Pan spread <input type="number" min="-540" max="540" step="1" bind:value={spread} aria-label="Pan spread" />°</label>
            <label>Tilt spread <input type="number" min="-270" max="270" step="1" bind:value={tiltSpread} aria-label="Tilt spread" />°</label>
          </div>
          <select bind:value={fanMode} aria-label="Fan mode"><option value="linear">linear</option><option value="from_center">from center</option></select>
        {:else if kind === 'gobo' || kind === 'prism'}
          <select bind:value={name} aria-label={kind === 'gobo' ? 'Gobo' : 'Prism'}>
            <option value="">— choose —</option>
            {#each names as n (n)}<option value={n}>{n}</option>{/each}
          </select>
          {#if !names.length}<span class="muted">The selected fixtures have no {kind}.</span>{/if}
        {:else if kind === 'zoom'}
          <label>Zoom <input type="number" min="1" max="90" bind:value={zoom} aria-label="Zoom degrees" />°</label>
        {:else if kind === 'strobe'}
          {#if mixed.strobe}{@render unit('strobe', 'Hz', 'frequencies in Hz')}{/if}
          {#if strobeAsDmx}
            <label>Strobe <input type="range" min="0" max="255" bind:value={strobeDmx} aria-label="Strobe DMX" /><input class="byte" type="number" min="0" max="255" bind:value={strobeDmx} aria-label="Strobe DMX value" /></label>
          {:else}
            <label>Strobe <input type="number" min="0.5" max="30" step="0.5" bind:value={hz} aria-label="Strobe Hz" /> Hz</label>
          {/if}
        {:else}
          <select bind:value={paletteId} aria-label="Palette">
            <option value={null}>— choose —</option>
            {#each show.palettes as p (p.id)}<option value={p.id}>{p.name} ({p.kind})</option>{/each}
          </select>
        {/if}
      </div>
      <p class="direct muted small">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 8.5 6.5 12 13 4.5"></path></svg>
        Changes go into the scene at once · Undo takes them back
      </p>
    </section>
  </div>
</div>
{/if}

{#if lights.members.length || scene.values.length}
<!-- The scene as values, or as every channel on faders: one view at a time. -->
<section class="card view" aria-label="Scene contents">
  <div class="row view-head">
    <div class="tabs" role="tablist" aria-label="View">
      <button role="tab" aria-selected={view === 'values'} class:on={view === 'values'} onclick={() => (view = 'values')}>Values</button>
      <button role="tab" aria-selected={view === 'channels'} class:on={view === 'channels'} onclick={() => (view = 'channels')}>DMX channels</button>
    </div>
    <span class="info" role="img" aria-label="How values combine" title="Values apply in order: on every channel the last one wins, whoever it goes to.">i</span>
  </div>
  {#if view === 'values'}
    <table>
      <tbody>
        {#each scene.values as sv, i (i)}
          {#if sv.value.type !== 'channel'}
            {@const cover = covers.get(i)}
            <tr class:covered={cover?.full} title={cover ? coveredBy(cover) : undefined}>
              <td>{describeTarget(show, sv.target)}{#if cover}<span class="cover" class:full={cover.full}>{cover.full ? 'no effect' : 'partly overridden'}</span>{/if}</td>
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
          <tr><td class="muted">No values yet: choose lights and set a value above.</td></tr>
        {/if}
      </tbody>
    </table>
  {:else}
    <SceneFaders functionId={f.id} onset={setChannel} ontry={(change) => { dragged = change; if (change) paused = false; }} />
  {/if}
</section>
{/if}

<SceneTiming {scene} onchange={update} />

{#if adding}
  <LightPickerDialog
    added={{ groups: memberGroups.map((g) => g.id), fixtures: lights.fixtures }}
    onconfirm={addLights}
    onclose={() => (adding = false)}
  />
{/if}

<style>
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .head .name { font-size: 18px; font-weight: 600; min-width: 0; flex: 0 1 280px; }
  .spacer { flex: 1; }
  .test { display: flex; align-items: stretch; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .test > button { border: none; border-radius: 0; min-height: 36px; padding: 0 14px; font-weight: 600; }
  .test .live { display: flex; align-items: center; gap: 6px; padding: 0 12px; border-left: 1px solid var(--border); font-size: 13px; cursor: pointer; }
  .menu-wrap { position: relative; }
  .more { width: 38px; min-height: 36px; padding: 0; font-size: 18px; line-height: 1; }
  .menu { position: absolute; right: 0; top: calc(100% + 6px); z-index: 20; min-width: 260px; display: flex; flex-direction: column; padding: 6px; background: var(--panel-2); border: 1px solid var(--border); border-radius: var(--radius); box-shadow: 0 10px 28px rgb(0 0 0 / 45%); }
  .menu button { text-align: left; border: none; background: none; padding: 8px 10px; }
  .menu button:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .menu hr { width: 100%; border: none; border-top: 1px solid var(--border); margin: 4px 0; }
  .danger-text { color: var(--danger, #e5484d); }
  .paused { margin: 6px 0 0; }
  .empty { margin-top: 14px; }
  /* Two steps side by side where there is room. The lights take the height of
     the panel next to them and scroll inside it: more fixtures never make the
     left column longer than the right one. */
  .compose { container-type: inline-size; margin-top: 14px; }
  .steps { display: flex; flex-direction: column; gap: 12px; }
  .card { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px 12px; background: var(--panel-2); min-width: 0; }
  .card h3 { display: flex; align-items: center; gap: 8px; margin: 0 0 10px; font-size: 15px; color: var(--text); }
  .step { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; border-radius: 50%; background: var(--accent); color: #fff; font-size: 12px; }
  .preset { display: flex; flex-direction: column; gap: 12px; }
  .preset h3, .preset p { margin: 0; }
  .target b { font-size: 15px; color: var(--accent); }
  .pick { display: flex; flex-direction: column; }
  @container (min-width: 640px) {
    .steps { display: grid; grid-template-columns: minmax(0, 1fr) minmax(300px, 380px); align-items: stretch; }
    /* No height of its own: it is sized by the row, which the panel sets. */
    .pick { height: 0; min-height: 100%; box-sizing: border-box; }
    .preset { min-height: 380px; }
  }
  .preset h3 .target { font-size: 13px; font-weight: 400; color: var(--muted); }
  .quick { display: flex; gap: 6px; }
  .quick button { flex: 1; }
  .pair { display: flex; gap: 8px; }
  .pair label { flex: 1; min-width: 0; }
  .direct { display: flex; align-items: center; gap: 6px; margin: auto 0 0; }
  .view { margin-top: 14px; }
  .view-head { align-items: center; margin-bottom: 8px; }
  .tabs { display: flex; padding: 3px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--panel); }
  .tabs button { border: none; background: none; padding: 6px 14px; font-weight: 600; color: var(--muted); }
  .tabs button.on { background: var(--panel-2); color: var(--text); box-shadow: 0 0 0 1px var(--border); }
  .info { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; border-radius: 50%; border: 1px solid var(--border); color: var(--muted); font-size: 12px; font-weight: 700; cursor: help; }
  .view table { width: 100%; }
  .attributes { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: 4px; }
  .attributes button { padding: 8px 4px; font-size: 13px; font-weight: 600; }
  .attributes button.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .unit { display: flex; gap: 4px; }
  .unit button { flex: 1; padding: 4px; font-size: 12px; }
  .unit button.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .small { font-size: 12px; }
  .control input[type='number'].byte { flex: 0 0 64px; }
  .control label input[type='range'] { flex: 1; min-width: 0; }
  .control { display: flex; flex-direction: column; gap: 8px; }
  .control input[type='range'] { width: 100%; }
  .control input[type='color'] { width: 100%; height: 44px; padding: 2px; cursor: pointer; }
  .control label { display: flex; align-items: center; gap: 8px; }
  .control input[type='number'] { flex: 1; min-width: 0; }
  .level { font-size: 28px; font-weight: 700; text-align: center; font-variant-numeric: tabular-nums; }
  .add-lights { margin-bottom: 8px; flex-wrap: wrap; }
  .wheel { display: flex; flex-wrap: wrap; gap: 6px; }
  .wheel .chip { width: 26px; height: 26px; padding: 0; border-radius: 6px; background: var(--c); border: 2px solid var(--border); cursor: pointer; }
  .wheel .chip:hover { border-color: var(--muted); }
  .wheel .chip.on { border-color: var(--text); box-shadow: 0 0 0 2px var(--accent); }
  .swatch { display: inline-block; width: 18px; height: 18px; border-radius: 4px; border: 1px solid var(--border); vertical-align: middle; margin-right: 6px; }
  .actions { text-align: right; white-space: nowrap; }
  tr.covered td { opacity: 0.55; }
  tr.covered td:nth-child(2) { text-decoration: line-through; }
  .cover { margin-left: 6px; padding: 0 6px; border-radius: 8px; font-size: 11px; border: 1px solid var(--border); color: var(--muted); white-space: nowrap; }
  .cover.full { border-style: dashed; }
</style>
