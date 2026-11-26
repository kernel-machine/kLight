<script lang="ts">
  // The faders of one fixture under a heading with its name.
  import type { Snippet } from 'svelte';

  let {
    name,
    patched = true,
    alt = false,
    wrap = false,
    children,
  }: {
    /** Empty for unpatched channels: the heading keeps its room, blank. */
    name: string;
    patched?: boolean;
    /** Alternate tint, to tell neighbouring fixtures apart. */
    alt?: boolean;
    /** Channels that do not fit in a line continue on the next one. */
    wrap?: boolean;
    children: Snippet;
  } = $props();
</script>

<div class="group" class:patched class:alt class:wrap role={name ? 'group' : undefined} aria-label={name || undefined}>
  <div class="fixture" title={name || undefined}><span>{name}</span></div>
  <div class="channels">{@render children()}</div>
</div>

<style>
  .group { display: flex; flex-direction: column; flex: none; gap: 3px; }
  .group.wrap { flex: 0 1 auto; max-width: 100%; min-width: 0; }
  /* As wide as the channels below, however long the name. */
  .fixture { width: 0; min-width: 100%; height: 20px; padding: 2px 4px; border-radius: var(--radius); font-size: 11px; font-weight: 600; line-height: 16px; white-space: nowrap; overflow: clip; }
  /* The name stays in sight while a row of faders scrolls sideways. */
  .fixture span { display: inline-block; position: sticky; left: 4px; max-width: 100%; overflow: hidden; text-overflow: ellipsis; vertical-align: top; }
  .patched .fixture { background: color-mix(in srgb, var(--accent) 22%, var(--panel)); }
  .patched.alt .fixture { background: color-mix(in srgb, var(--ok) 22%, var(--panel)); }
  .channels { display: flex; flex: 1; gap: 4px; }
  .wrap .channels { flex-wrap: wrap; }
</style>
