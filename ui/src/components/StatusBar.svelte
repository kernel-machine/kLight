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
  // MIDI/OSC activity lamp: lit for a moment whenever a controller's message
  // counter grows between two status updates.
  const LAMP_MS = 600;
  let lampOn = $state(false);
  let lastEvents = new Map<string, number>();
  let lampTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const next = new Map((st?.inputs ?? []).map((i) => [i.name, i.events]));
    const active = [...next].some(([name, n]) => n > (lastEvents.get(name) ?? n));
    lastEvents = next;
    if (active) {
      lampOn = true;
      clearTimeout(lampTimer);
      lampTimer = setTimeout(() => (lampOn = false), LAMP_MS);
    }
  });
  // Only on teardown: this must not run on every status update, or it would
  // cancel the pending "lamp off" and leave the lamp on.
  $effect(() => () => clearTimeout(lampTimer));

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
    <span class="badge" title="Lights up when a controller sends MIDI/OSC messages" aria-label="Input activity">
      <i class="lamp" class:on={lampOn}></i> MIDI
    </span>
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
  .lamp { display: inline-block; width: 9px; height: 9px; border-radius: 50%; background: var(--border); margin-right: 4px; vertical-align: middle; transition: background 0.1s, box-shadow 0.1s; }
  .lamp.on { background: var(--ok, #3ecf6e); box-shadow: 0 0 6px var(--ok, #3ecf6e); }
  footer { display: flex; gap: 6px; flex-wrap: wrap; padding: 6px 12px; background: var(--panel); border-top: 1px solid var(--border); font-size: 12px; }
</style>
