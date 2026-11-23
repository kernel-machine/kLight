<script lang="ts">
  import { app, run } from '../lib/app.svelte';
  import { nextId } from '../lib/show';
  import type { ButtonAction, SliderMode, VirtualConsole, Widget, WidgetKind } from '../lib/protocol';
  import VcWidget from '../components/VcWidget.svelte';

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

  function hexToRgb8(hex: string) {
    const n = Number.parseInt(hex.slice(1), 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
  }
  const rgbHex = (c: { r: number; g: number; b: number } | null) =>
    c ? `#${[c.r, c.g, c.b].map((v) => v.toString(16).padStart(2, '0')).join('')}` : '#4f9dff';
</script>

<div class="vc">
  {#if design}
    <div class="toolbar panel row">
      <span class="muted">Design:</span>
      <button onclick={() => add({ type: 'button', action: 'Toggle' }, 'Button')}>Button</button>
      <button onclick={() => add({ type: 'slider', mode: 'Playback' }, 'Fader', 70, 220)}>Slider</button>
      <button onclick={() => add({ type: 'knob' }, 'Knob', 100, 130)}>Knob</button>
      <button onclick={() => add({ type: 'cue_list' }, 'Cue list', 140, 90)}>Cue list</button>
      <button onclick={() => add({ type: 'speed_dial' }, 'Tempo', 100, 80)}>Speed dial</button>
      <button onclick={() => add({ type: 'frame', pages: 1 }, 'Frame', 360, 240)}>Frame</button>
      <button onclick={() => add({ type: 'solo_frame' }, 'Solo frame', 360, 240)}>Solo frame</button>
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
          <label>Function <select value={w.function ?? ''} onchange={(e) => update(w.id, (x) => ({ ...x, function: e.currentTarget.value === '' ? null : Number(e.currentTarget.value) }))}>
            <option value="">— none —</option>
            {#each show.functions as f (f.id)}<option value={f.id}>{f.name}</option>{/each}
          </select></label>
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
  .props label { display: flex; flex-direction: column; gap: 4px; }
</style>
