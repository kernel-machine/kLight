<script lang="ts">
  // Small icon of what a channel controls (see lib/channelIcon.ts). Purely
  // decorative: the channel name is always shown next to it, and the name of
  // the function is a tooltip.
  import { channelIcon, channelIconLabel } from '../lib/channelIcon';
  import type { Attribute } from '../lib/protocol';

  let { attribute, size = 16 }: { attribute: Attribute; size?: number } = $props();
  const icon = $derived(channelIcon(attribute));

  // Six slices of the color wheel, from the top.
  const SLICES = ['#e5392f', '#ffd400', '#27b34b', '#19c3d9', '#2f6bf0', '#e0258f'].map((fill, k) => {
    const at = (deg: number) => {
      const r = (deg - 90) * (Math.PI / 180);
      return `${(8 + 6.8 * Math.cos(r)).toFixed(2)} ${(8 + 6.8 * Math.sin(r)).toFixed(2)}`;
    };
    return { fill, d: `M8 8 L${at(k * 60)} A6.8 6.8 0 0 1 ${at(k * 60 + 60)} Z` };
  });
  // Holes of a gobo disc.
  const HOLES = Array.from({ length: 6 }, (_, k) => {
    const r = (k * 60) * (Math.PI / 180);
    return { x: 7.5 + 3 * Math.cos(r), y: 8.5 + 3 * Math.sin(r) };
  });
</script>

<span class="ci" title={channelIconLabel(attribute)} aria-hidden="true" style="width:{size}px; height:{size}px">
  <svg viewBox="0 0 16 16" width={size} height={size} fill="none" stroke-linecap="round" stroke-linejoin="round">
    {#if icon.type === 'color'}
      <rect x="0.75" y="0.75" width="14.5" height="14.5" rx="3" fill={icon.fill} stroke="#0006" stroke-width="1" />
      <text x="8" y="11.4" text-anchor="middle" font-size={icon.letter.length > 1 ? 7.5 : 10} font-weight="800" font-family="system-ui, sans-serif" fill={icon.ink}>{icon.letter}</text>
    {:else if icon.type === 'bulb'}
      <path d="M8 1.5a4.6 4.6 0 0 0-2.7 8.3c.4.3.7.8.7 1.3v.9h4v-.9c0-.5.3-1 .7-1.3A4.6 4.6 0 0 0 8 1.5Z" fill="#ffd54a" stroke="#b8860b" stroke-width="1" />
      <path d="M6.3 13.6h3.4M7 15h2" stroke="#8a93a3" stroke-width="1.2" />
    {:else if icon.type === 'pan'}
      <path d="M1.5 8h13M4.7 4.8 1.5 8l3.2 3.2M11.3 4.8 14.5 8l-3.2 3.2" stroke="#3b82f6" stroke-width="1.8" />
    {:else if icon.type === 'tilt'}
      <path d="M8 1.5v13M4.8 4.7 8 1.5l3.2 3.2M4.8 11.3 8 14.5l3.2-3.2" stroke="#3b82f6" stroke-width="1.8" />
    {:else if icon.type === 'strobe'}
      <path d="M9.4 0.9 3.2 9h4l-1 6.1L12.8 6.8H8.7Z" fill="#ffc107" stroke="#c77700" stroke-width="1" />
    {:else if icon.type === 'wheel'}
      {#each SLICES as s (s.fill)}<path d={s.d} fill={s.fill} />{/each}
      <circle cx="8" cy="8" r="6.8" stroke="#0007" stroke-width="0.8" />
      <circle cx="8" cy="8" r="1.7" fill="var(--panel, #222)" stroke="#0007" stroke-width="0.8" />
    {:else if icon.type === 'gobo'}
      <circle cx="7.5" cy="8.5" r="5.8" fill="#aab4c3" stroke="#5f6b7d" stroke-width="1" />
      {#each HOLES as h, i (i)}<circle cx={h.x} cy={h.y} r="1.15" fill="var(--panel, #222)" />{/each}
      <circle cx="7.5" cy="8.5" r="1.15" fill="var(--panel, #222)" />
      {#if icon.rotating}
        <path d="M10.4 1.8a6.4 6.4 0 0 1 4.2 4.1M14.9 3.4l-.3 2.8-2.7-.6" stroke="#3b82f6" stroke-width="1.5" />
      {/if}
    {:else if icon.type === 'speed'}
      <circle cx="8" cy="9" r="5.6" stroke="currentColor" stroke-width="1.4" />
      <path d="M8 9V5.6M8 9l2.2 1.4M6.5 1.7h3" stroke="currentColor" stroke-width="1.4" />
    {:else}
      <path d="m8 1.6 1.9 4 4.3.5-3.2 3 .9 4.3L8 11.1l-3.9 2.3.9-4.3-3.2-3 4.3-.5Z" fill="#d4a017" stroke="#8a6a00" stroke-width="0.9" />
    {/if}
  </svg>
</span>

<style>
  .ci { display: inline-flex; flex: none; vertical-align: middle; line-height: 0; }
</style>
