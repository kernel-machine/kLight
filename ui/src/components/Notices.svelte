<script lang="ts">
  import { app } from '../lib/app.svelte';
</script>

<div class="notices" aria-live="polite">
  {#each app.notices as n (n.id)}
    <div class="notice {n.kind}">{n.text}</div>
  {/each}
</div>

<style>
  /* Notices hold no controls: clicks go through to the screen under them. */
  .notices { pointer-events: none; position: fixed; right: 132px; bottom: 48px; display: flex; flex-direction: column; gap: 6px; z-index: 50; max-width: 420px; }
  .notice { background: var(--panel); border: 1px solid var(--border); border-left: 4px solid var(--accent); padding: 8px 12px; border-radius: var(--radius); box-shadow: 0 4px 16px #0006; }
  .notice.error { border-left-color: var(--danger); }
  .notice.warning { border-left-color: var(--warn); }
</style>
