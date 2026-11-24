<script lang="ts">
  // Pick a named range of a channel (a color of the wheel, a gobo, a shutter
  // mode...). The caller sets the channel to the preset's value, so a fader
  // moves straight to it.
  import { presetAt, presetSwatch } from '../lib/channelIcon';
  import type { ChannelPreset } from '../lib/protocol';

  let {
    title,
    presets,
    value,
    x,
    top,
    bottom,
    onpick,
    onclose,
  }: {
    title: string;
    presets: ChannelPreset[];
    /** Current DMX value of the channel (its preset is highlighted). */
    value: number;
    /** Where the button that opened the list is (viewport pixels). */
    x: number;
    top: number;
    bottom: number;
    onpick: (value: number) => void;
    onclose: () => void;
  } = $props();

  const WIDTH = 240;
  const current = $derived(presetAt(presets, value));
  // Stay inside the window: open to the left / upwards when near an edge.
  const left = $derived(Math.max(8, Math.min(x, window.innerWidth - WIDTH - 8)));
  // Below the button, or above it (never over it) near the bottom.
  const below = $derived(bottom < window.innerHeight * 0.55);
  let root = $state<HTMLElement>();

  $effect(() => {
    // The current preset, else the first one.
    (root?.querySelector<HTMLElement>('button.on') ?? root?.querySelector<HTMLElement>('button'))?.focus();
  });

  function outside(e: PointerEvent) {
    if (root && !root.contains(e.target as Node)) onclose();
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div
  class="picker panel"
  bind:this={root}
  role="listbox"
  aria-label="{title} presets"
  style="left:{left}px; width:{WIDTH}px; {below ? `top:${bottom + 4}px; max-height:${window.innerHeight - bottom - 16}px` : `bottom:${window.innerHeight - top + 4}px; max-height:${top - 16}px`}"
>
  <div class="title muted">{title}</div>
  {#each presets as p, i (i)}
    {@const swatch = presetSwatch(p.kind)}
    <button class:on={i === current} role="option" aria-selected={i === current} onclick={() => { onpick(p.value); onclose(); }}>
      <span class="sw" class:empty={!swatch} style={swatch ? `background:${swatch}` : ''}></span>
      <span class="name">{p.name}</span>
      <span class="range muted">{p.min === p.max ? p.min : `${p.min}–${p.max}`}</span>
    </button>
  {/each}
</div>

<style>
  .picker { position: fixed; z-index: 60; overflow: auto; padding: 4px; box-shadow: 0 8px 32px #000a; display: flex; flex-direction: column; gap: 2px; }
  .title { font-size: 11px; padding: 2px 6px; }
  button { display: flex; align-items: center; gap: 8px; text-align: left; border: none; background: transparent; padding: 5px 6px; border-radius: 4px; }
  button:hover { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  button.on { background: color-mix(in srgb, var(--accent) 30%, transparent); font-weight: 700; }
  .sw { width: 16px; height: 16px; border-radius: 4px; flex: none; border: 1px solid #0006; }
  .sw.empty { background: transparent; border-style: dashed; border-color: var(--border); }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .range { font-size: 11px; font-variant-numeric: tabular-nums; }
</style>
