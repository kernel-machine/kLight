<script lang="ts">
  // Output values of a universe; fixtures are separated by color bands and
  // every channel says what it is.
  import { app, client, pref, setPref } from '../lib/app.svelte';
  import { channelMap, groupColor, groupsOf, GROUP_COLORS } from '../lib/show';
  import ChannelIcon from './ChannelIcon.svelte';

  let { onclose }: { onclose: () => void } = $props();
  let universe = $state(app.s.show?.universes[0]?.id ?? 1);
  let patchedOnly = $state(false);

  // Resizable: drag the handle in the top-left corner (the panel is anchored
  // bottom-right). The size is remembered by this browser.
  const MIN_W = 320;
  const MIN_H = 160;
  const maxW = () => Math.max(MIN_W, window.innerWidth - 24);
  const maxH = () => Math.max(MIN_H, window.innerHeight - 60);
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
  const saved = pref<{ w: number; h: number; right?: number; bottom?: number } | null>('monitor.size', null);
  const initialW = clamp(saved?.w ?? window.innerWidth - 144, MIN_W, maxW());
  const initialH = clamp(saved?.h ?? Math.round(window.innerHeight * 0.55), MIN_H, maxH());
  let width = $state(initialW);
  let height = $state(initialH);
  // Position: distance from the right and bottom edges of the window.
  let right = $state(clamp(saved?.right ?? 132, 0, Math.max(0, window.innerWidth - initialW)));
  let bottom = $state(clamp(saved?.bottom ?? 44, 0, Math.max(0, window.innerHeight - initialH)));
  const keepOnScreen = () => {
    right = clamp(right, 0, Math.max(0, window.innerWidth - width));
    bottom = clamp(bottom, 0, Math.max(0, window.innerHeight - height));
  };
  const save = () => setPref('monitor.size', { w: width, h: height, right, bottom });

  let moving: { x: number; y: number; right: number; bottom: number } | null = null;
  function startMove(e: PointerEvent) {
    moving = { x: e.clientX, y: e.clientY, right, bottom };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!moving) return;
    right = clamp(moving.right - (e.clientX - moving.x), 0, Math.max(0, window.innerWidth - width));
    bottom = clamp(moving.bottom - (e.clientY - moving.y), 0, Math.max(0, window.innerHeight - height));
  }
  function endMove() {
    if (!moving) return;
    moving = null;
    save();
  }

  let resizing: { x: number; y: number; w: number; h: number } | null = null;

  function startResize(e: PointerEvent) {
    resizing = { x: e.clientX, y: e.clientY, w: width, h: height };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function resize(e: PointerEvent) {
    if (!resizing) return;
    // Dragging up/left makes the panel bigger.
    width = clamp(resizing.w + (resizing.x - e.clientX), MIN_W, maxW());
    height = clamp(resizing.h + (resizing.y - e.clientY), MIN_H, maxH());
    keepOnScreen();
  }
  function endResize() {
    if (!resizing) return;
    resizing = null;
    save();
  }

  $effect(() => {
    return client.watchDmx([universe]);
  });
  const show = $derived(app.s.show!);
  const data = $derived(app.s.dmx.get(universe) ?? []);
  const map = $derived(channelMap(show, universe));
  const band = (fixture_index: number) => GROUP_COLORS[fixture_index % GROUP_COLORS.length];
  const cells = $derived(Array.from({ length: 512 }, (_, i) => i).filter((i) => !patchedOnly || map[i]));
</script>

<div class="monitor panel" style="width:{width}px; height:{height}px; right:{right}px; bottom:{bottom}px">
  <div
    class="grip"
    role="separator"
    aria-label="Resize monitor"
    title="Drag to resize"
    onpointerdown={startResize}
    onpointermove={resize}
    onpointerup={endResize}
    onpointercancel={endResize}
  ></div>
  <div class="row">
    <h2
      class="title"
      title="Drag to move"
      onpointerdown={startMove}
      onpointermove={move}
      onpointerup={endMove}
      onpointercancel={endMove}
    >DMX monitor</h2>
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
        <span class="ch">{#if info}<ChannelIcon attribute={info.attribute} size={11} />{' '}{/if}{info?.channel_name ?? ''}</span>
        <span class="v">{v}</span>
        <div class="bar"></div>
      </div>
    {/each}
  </div>
</div>

<style>
  .title { cursor: move; user-select: none; touch-action: none; margin: 0; padding: 2px 6px; }
  .monitor { position: fixed; overflow: auto; z-index: 40; box-shadow: 0 8px 32px #000a; max-width: calc(100vw - 24px); max-height: calc(100vh - 60px); }
  .grip { position: sticky; top: 0; left: 0; float: left; width: 16px; height: 16px; margin: -6px 0 0 -6px; cursor: nwse-resize; touch-action: none; z-index: 1; border-top: 3px solid var(--muted); border-left: 3px solid var(--muted); border-radius: 3px 0 0 0; opacity: 0.7; }
  .grip:hover { opacity: 1; border-color: var(--accent); }
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
