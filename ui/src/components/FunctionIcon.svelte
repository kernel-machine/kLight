<script lang="ts">
  // The mark in front of a function's name: the color of a scene that sets one,
  // otherwise an icon of its type, in the same box so that lists stay aligned.
  // Purely decorative: the name of the function is always next to it, and its
  // type is a tooltip.
  import { app } from '../lib/app.svelte';
  import { FUNCTION_CATEGORIES, colorToHex, functionColor } from '../lib/show';
  import type { Function as Fn } from '../lib/protocol';

  let { f, size = 16 }: { f: Fn; size?: number } = $props();

  const color = $derived(functionColor(app.s.show!, f));
  const label = $derived(FUNCTION_CATEGORIES.find((c) => c.id === f.kind.type)?.short);
</script>

<span class="fi" data-icon={color ? 'color' : f.kind.type} title={label} aria-hidden="true" style="width:{size}px; height:{size}px">
  <svg viewBox="0 0 16 16" width={size} height={size} fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
    {#if color}
      <rect x="0.75" y="0.75" width="14.5" height="14.5" rx="3" fill={colorToHex(color)} stroke="var(--border)" stroke-width="1" />
    {:else if f.kind.type === 'scene'}
      <circle cx="8" cy="8" r="3" />
      <path d="M8 1.2v1.6M8 13.2v1.6M1.2 8h1.6M13.2 8h1.6M3.2 3.2l1.1 1.1M11.7 11.7l1.1 1.1M3.2 12.8l1.1-1.1M11.7 4.3l1.1-1.1" />
    {:else if f.kind.type === 'chaser'}
      <path d="m2.5 3.5 4.5 4.5-4.5 4.5M8.5 3.5 13 8l-4.5 4.5" />
    {:else if f.kind.type === 'cue_list'}
      <path d="M6 3.5h8.5M6 8h8.5M6 12.5h8.5M2 3.5h.5M2 8h.5M2 12.5h.5" />
    {:else if f.kind.type === 'movement_effect'}
      <path d="M8 8c-1.3-1.9-2.4-3-3.9-3a3 3 0 0 0 0 6c1.5 0 2.6-1.1 3.9-3s2.4-3 3.9-3a3 3 0 0 1 0 6c-1.5 0-2.6-1.1-3.9-3Z" />
    {:else if f.kind.type === 'collection'}
      <path d="M8 1.5 14.5 4.8 8 8.1 1.5 4.8ZM1.8 8.2 8 11.3l6.2-3.1M1.8 11.4 8 14.5l6.2-3.1" />
    {:else}
      <g fill="currentColor" stroke="none">
        {#each [1.5, 6.5, 11.5] as y (y)}
          {#each [1.5, 6.5, 11.5] as x (x)}<rect {x} {y} width="3" height="3" rx="0.8" />{/each}
        {/each}
      </g>
    {/if}
  </svg>
</span>

<style>
  .fi { display: inline-flex; flex: none; line-height: 0; color: var(--muted); }
</style>
