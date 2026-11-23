<script lang="ts">
  import { app } from '../lib/app.svelte';

  const st = $derived(app.s.status);
  const rt = $derived(app.s.runtime?.state);
  const conn = $derived(app.s.connection);

  function ago(ms: number | null | undefined): string {
    if (!ms) return 'never';
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
    return s < 60 ? `${s}s ago` : `${Math.round(s / 60)} min ago`;
  }
  const outClass = (state: string) => (state === 'ok' ? 'ok' : state === 'connecting' ? 'warn' : 'danger');
</script>

<footer>
  <span class="badge {conn === 'connected' ? 'ok' : 'danger'}">Engine: {conn}</span>
  {#if st}
    {#each st.outputs as o (o.universe)}
      <span class="badge {outClass(o.state)}" title={o.last_error ?? ''}>
        U{o.universe} {o.kind}: {o.state}{o.hardware_buffer ? '' : ' · no hardware buffer'}
      </span>
    {:else}
      <span class="badge warn">No outputs assigned</span>
    {/each}
    <span class="badge">{st.inputs.filter((i) => i.connected).length} controllers</span>
    <span class="badge {st.jitter.p99_us > 5000 ? 'warn' : ''}" title="min/avg/max {st.jitter.min_us}/{st.jitter.avg_us}/{st.jitter.max_us} µs">
      Jitter p99 {(st.jitter.p99_us / 1000).toFixed(1)} ms
    </span>
    <span class="badge {st.autosave_error ? 'danger' : ''}" title={st.autosave_error ?? ''}>Saved {ago(st.last_autosave_ms)}</span>
    <span class="badge">{st.clients} clients</span>
  {/if}
  {#if rt}
    <span class="badge {rt.mode === 'Show' ? 'warn' : ''}">{rt.mode}</span>
    <span class="badge {rt.tempo.freewheel ? 'warn' : ''}">
      {rt.tempo.bpm.toFixed(1)} BPM · {rt.tempo.source}{rt.tempo.freewheel ? ' (source lost, freewheel)' : ''}
    </span>
  {/if}
  {#if app.s.runtime?.warnings.length}
    <span class="badge warn" title={app.s.runtime.warnings.join('\n')}>{app.s.runtime.warnings.length} warnings</span>
  {/if}
</footer>

<style>
  footer { display: flex; gap: 6px; flex-wrap: wrap; padding: 6px 12px; background: var(--panel); border-top: 1px solid var(--border); font-size: 12px; }
</style>
