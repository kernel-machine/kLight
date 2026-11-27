<script lang="ts">
  // Header shared by the FX editors: name, Start/Stop to try the function on
  // the lights, and the rare actions in a menu.
  import { app, run } from '../lib/app.svelte';
  import type { Function as Fn } from '../lib/protocol';

  let {
    f,
    badge,
    nameLabel,
    deleteLabel,
    onrename,
    onduplicate,
    ondelete,
  }: {
    f: Fn;
    badge: string;
    nameLabel: string;
    deleteLabel: string;
    onrename: (name: string) => void;
    onduplicate: () => void;
    ondelete: () => void;
  } = $props();

  const running = $derived((app.s.runtime?.state.active ?? []).some((a) => a.id === f.id));
  const toggleRun = () => run(running ? { type: 'stop_function', id: f.id } : { type: 'start_function', id: f.id });

  let menuOpen = $state(false);
  let menuWrap = $state<HTMLElement>();
  $effect(() => {
    void f.id;
    menuOpen = false;
  });
  const act = (fn: () => void) => () => {
    menuOpen = false;
    fn();
  };
  function outside(e: MouseEvent) {
    if (menuOpen && menuWrap && !menuWrap.contains(e.target as Node)) menuOpen = false;
  }
</script>

<svelte:window onclick={outside} />

<div class="head">
  <input class="name" value={f.name} aria-label={nameLabel} onchange={(e) => onrename(e.currentTarget.value)} />
  <span class="badge">{badge}</span>
  <span class="spacer"></span>
  {#if running}
    <button class="primary run" onclick={toggleRun}>Stop</button>
  {:else}
    <button class="run" onclick={toggleRun} title="Plays it on the lights">Start</button>
  {/if}
  <div class="menu-wrap" bind:this={menuWrap}>
    <button class="more" aria-label="More actions" aria-haspopup="menu" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>⋯</button>
    {#if menuOpen}
      <div class="menu" role="menu" tabindex="-1" onkeydown={(e) => { if (e.key === 'Escape') menuOpen = false; }}>
        <button role="menuitem" onclick={act(onduplicate)}>Duplicate</button>
        <hr />
        <button role="menuitem" class="danger-text" onclick={act(ondelete)}>{deleteLabel}</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; }
  .head .name { font-size: 18px; font-weight: 600; min-width: 0; flex: 0 1 280px; }
  .spacer { flex: 1; }
  .run { min-height: 36px; padding: 0 16px; }
  .menu-wrap { position: relative; }
  .more { width: 38px; min-height: 36px; padding: 0; font-size: 18px; line-height: 1; }
  .menu { position: absolute; right: 0; top: calc(100% + 6px); z-index: 20; min-width: 220px; display: flex; flex-direction: column; padding: 6px; background: var(--panel-2); border: 1px solid var(--border); border-radius: var(--radius); box-shadow: 0 10px 28px rgb(0 0 0 / 45%); }
  .menu button { text-align: left; border: none; background: none; padding: 8px 10px; }
  .menu button:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .menu hr { width: 100%; border: none; border-top: 1px solid var(--border); margin: 4px 0; }
  .danger-text { color: var(--danger, #e5484d); }
</style>
