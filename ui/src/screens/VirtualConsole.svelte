<script lang="ts">
  import { app, client, notify, run } from '../lib/app.svelte';
  import { colorToHex, describeInputMessage, frameFromFunctions, functionColor, nextId } from '../lib/show';
  import type { ButtonAction, SliderMode, VirtualConsole, Widget, WidgetKind } from '../lib/protocol';
  import VcWidget from '../components/VcWidget.svelte';
  import FrameFromScenes from '../components/FrameFromScenes.svelte';

  const show = $derived(app.s.show!);
  const vc = $derived(show.virtual_console);
  const design = $derived(app.s.runtime?.state.mode === 'Design' && app.s.permission === 'full');
  let selectedId = $state<number | null>(null);

  function allWidgets(ws: Widget[]): Widget[] {
    return ws.flatMap((w) => [w, ...allWidgets(w.children)]);
  }
  const selected = $derived(allWidgets(vc.widgets).find((w) => w.id === selectedId) ?? null);

  function mapTree(ws: Widget[], f: (w: Widget) => Widget | null): Widget[] {
    return ws.flatMap((w) => {
      const r = f(w);
      return r ? [{ ...r, children: mapTree(r.children, f) }] : [];
    });
  }

  function saveVc(widgets: Widget[]) {
    const console: VirtualConsole = { widgets };
    return run({ type: 'set_virtual_console', console });
  }

  function update(id: number, f: (w: Widget) => Widget) {
    saveVc(mapTree(vc.widgets, (w) => (w.id === id ? f(w) : w)));
  }

  function add(kind: WidgetKind, label: string, width = 100, height = 70) {
    const id = nextId(allWidgets(vc.widgets));
    const w: Widget = {
      id,
      kind,
      props: { x: 20 + (id % 8) * 20, y: 20 + (id % 8) * 20, width, height, label, color: null, page: 0 },
      function: null,
      children: [],
    };
    // Add into the selected frame if any.
    if (selected && (selected.kind.type === 'frame' || selected.kind.type === 'solo_frame')) {
      update(selected.id, (f) => ({ ...f, children: [...f.children, { ...w, props: { ...w.props, x: 10, y: 24 } }] }));
    } else {
      saveVc([...vc.widgets, w]);
    }
    selectedId = id;
  }

  // ---- MIDI mapping of the selected widget (same mappings as the Devices screen)
  const mappingsOf = (id: number) => show.input_mappings.filter((m) => m.target.type === 'widget' && m.target.id === id);
  let learningFor = $state<number | null>(null);

  async function learnMidi(w: Widget) {
    if (learningFor !== null) return;
    learningFor = w.id;
    notify('Move or press a control on the controller…', 'info');
    try {
      const l = await client.learn();
      // Read the latest show: mappings may have changed while waiting.
      const current = app.s.show?.input_mappings ?? [];
      const duplicate = current.some(
        (m) => m.device === l.device && JSON.stringify(m.message) === JSON.stringify(l.message) && m.target.type === 'widget' && m.target.id === w.id,
      );
      if (duplicate) {
        notify('That control is already mapped to this widget', 'warning');
        return;
      }
      const mapping = { id: nextId(current), device: l.device, message: l.message, target: { type: 'widget', id: w.id } } as const;
      if (await run({ type: 'set_input_mappings', mappings: [...current, mapping] })) {
        notify(`Mapped ${describeInputMessage(l.message)} on ${l.device}`, 'info');
      }
    } finally {
      learningFor = null;
    }
  }

  function unmap(mappingId: number) {
    run({ type: 'set_input_mappings', mappings: show.input_mappings.filter((m) => m.id !== mappingId) });
  }

  let framePicker = $state(false);

  function createSceneFrame(opts: { ids: number[]; solo: boolean; name: string }) {
    framePicker = false;
    const id = nextId(allWidgets(vc.widgets));
    const frame = frameFromFunctions(show, opts.ids, { ...opts, firstId: id, x: 20 + (id % 8) * 20, y: 20 + (id % 8) * 20 });
    saveVc([...vc.widgets, frame]);
    selectedId = id;
  }

  /** Label a button gets when created (see the toolbar). */
  const DEFAULT_BUTTON_LABEL = 'Button';

  /**
   * Assigns a function to a widget. A button that still has its default label
   * and no color of its own takes the function's name and, for a scene that
   * sets a color, that color; customised buttons keep theirs.
   */
  function assignFunction(w: Widget, value: string) {
    const id = value === '' ? null : Number(value);
    const f = show.functions.find((x) => x.id === id);
    update(w.id, (x) => {
      const next: Widget = { ...x, function: id };
      if (f && x.kind.type === 'button') {
        const color = functionColor(show, f);
        next.props = {
          ...x.props,
          label: x.props.label === DEFAULT_BUTTON_LABEL ? f.name : x.props.label,
          color: x.props.color === null && color ? hexToRgb8(colorToHex(color)) : x.props.color,
        };
      }
      return next;
    });
  }

  function hexToRgb8(hex: string) {
    const n = Number.parseInt(hex.slice(1), 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }
  const rgbHex = (c: { r: number; g: number; b: number } | null) =>
    c ? `#${[c.r, c.g, c.b].map((v) => v.toString(16).padStart(2, '0')).join('')}` : '#4f9dff';
</script>

<div class="vc">
  {#if framePicker}
    <FrameFromScenes oncreate={createSceneFrame} onclose={() => (framePicker = false)} />
  {/if}
  {#if design}
    <div class="toolbar panel row">
      <span class="muted">Design:</span>
      <button onclick={() => add({ type: 'button', action: 'Toggle' }, DEFAULT_BUTTON_LABEL)}>Button</button>
      <button onclick={() => add({ type: 'slider', mode: 'Playback' }, 'Fader', 70, 220)}>Slider</button>
      <button onclick={() => add({ type: 'knob' }, 'Knob', 100, 130)}>Knob</button>
      <button onclick={() => add({ type: 'cue_list' }, 'Cue list', 140, 90)}>Cue list</button>
      <button onclick={() => add({ type: 'speed_dial' }, 'Tempo', 100, 80)}>Speed dial</button>
      <button onclick={() => add({ type: 'frame', pages: 1 }, 'Frame', 360, 240)}>Frame</button>
      <button onclick={() => add({ type: 'solo_frame' }, 'Solo frame', 360, 240)}>Solo frame</button>
      <button onclick={() => (framePicker = true)} disabled={!show.functions.length} title="Create a frame with one button per chosen function">Frame from functions…</button>
      <button onclick={() => add({ type: 'label' }, 'Label', 140, 30)}>Label</button>
      <button onclick={() => add({ type: 'clock' }, 'Clock', 140, 60)}>Clock</button>
    </div>
  {:else}
    <div class="muted hint">Show mode: widgets are live. Switch to Design mode to edit the layout.</div>
  {/if}
  <div class="body">
    <div class="canvas" role="presentation" onpointerdown={() => (selectedId = null)}>
      {#each vc.widgets as w (w.id)}
        <VcWidget
          widget={w}
          {design}
          selected={selectedId}
          onselect={(id) => (selectedId = id)}
          onmove={(id, dx, dy) => update(id, (x) => ({ ...x, props: { ...x.props, x: Math.max(0, x.props.x + dx), y: Math.max(0, x.props.y + dy) } }))}
        />
      {/each}
    </div>
    {#if design && selected}
      {@const w = selected}
      <aside class="panel col props">
        <h2>Widget {w.id}</h2>
        <label>Label <input value={w.props.label} onchange={(e) => update(w.id, (x) => ({ ...x, props: { ...x.props, label: e.currentTarget.value } }))} /></label>
        <div class="row">
          <label>W <input type="number" value={w.props.width} style="width:70px" onchange={(e) => update(w.id, (x) => ({ ...x, props: { ...x.props, width: Number(e.currentTarget.value) } }))} /></label>
          <label>H <input type="number" value={w.props.height} style="width:70px" onchange={(e) => update(w.id, (x) => ({ ...x, props: { ...x.props, height: Number(e.currentTarget.value) } }))} /></label>
        </div>
        <label>Color <input type="color" value={rgbHex(w.props.color)} onchange={(e) => update(w.id, (x) => ({ ...x, props: { ...x.props, color: hexToRgb8(e.currentTarget.value) } }))} /></label>
        {#if w.kind.type === 'frame' || w.kind.type === 'solo_frame'}
          <label>Type <select value={w.kind.type} onchange={(e) => update(w.id, (x) => ({ ...x, kind: e.currentTarget.value === 'solo_frame' ? { type: 'solo_frame' } : { type: 'frame', pages: 1 } }))}>
            <option value="frame">Frame</option><option value="solo_frame">Solo frame (one at a time)</option></select></label>
        {/if}
        {#if w.kind.type === 'button'}
          {@const k = w.kind}
          <label>Action <select value={k.action} onchange={(e) => update(w.id, (x) => ({ ...x, kind: { ...k, action: e.currentTarget.value as ButtonAction } }))}>
            <option>Toggle</option><option>Flash</option><option>Blackout</option><option>StopAll</option><option>TapTempo</option></select></label>
        {/if}
        {#if w.kind.type === 'slider'}
          {@const k = w.kind}
          <label>Mode <select value={k.mode} onchange={(e) => update(w.id, (x) => ({ ...x, kind: { ...k, mode: e.currentTarget.value as SliderMode } }))}>
            <option>Playback</option><option>Submaster</option><option>GrandMaster</option></select></label>
        {/if}
        {#if !['frame', 'solo_frame', 'label', 'clock', 'speed_dial'].includes(w.kind.type)}
          <label>Function <select value={w.function ?? ''} onchange={(e) => assignFunction(w, e.currentTarget.value)}>
            <option value="">— none —</option>
            {#each show.functions as f (f.id)}<option value={f.id}>{f.name}</option>{/each}
          </select></label>
        {/if}
        {#if !['frame', 'solo_frame', 'label', 'clock'].includes(w.kind.type)}
          <div class="midi col">
            <b>MIDI</b>
            {#each mappingsOf(w.id) as m (m.id)}
              <div class="row"><span class="grow">{describeInputMessage(m.message)} <span class="muted">· {m.device}</span></span><button aria-label="Remove mapping" onclick={() => unmap(m.id)}>×</button></div>
            {:else}
              <span class="muted">Not mapped</span>
            {/each}
            <button disabled={learningFor !== null} onclick={() => learnMidi(w)}>
              {learningFor === w.id ? 'Waiting for the controller…' : 'MIDI learn'}
            </button>
          </div>
        {/if}
        <button class="danger" onclick={() => { saveVc(mapTree(vc.widgets, (x) => (x.id === w.id ? null : x))); selectedId = null; }}>Delete widget</button>
      </aside>
    {/if}
  </div>
</div>

<style>
  .vc { display: flex; flex-direction: column; gap: 8px; height: 100%; }
  .hint { font-size: 12px; }
  .body { display: flex; gap: 12px; flex: 1; min-height: 480px; }
  .canvas { position: relative; flex: 1; background-image: radial-gradient(var(--border) 1px, transparent 1px); background-size: 20px 20px; border: 1px solid var(--border); border-radius: var(--radius); overflow: auto; min-height: 480px; }
  .props { width: 260px; flex-shrink: 0; }
  .midi { gap: 6px; padding: 8px; border: 1px solid var(--border); border-radius: var(--radius); }
  .grow { flex: 1; }
  .props label { display: flex; flex-direction: column; gap: 4px; }
</style>
