<script lang="ts">
  // Blackout and grand master: always reachable.
  import { app, client, notify, run } from '../lib/app.svelte';
  import { enterDesignMode } from '../lib/designMode';
  import { createBeatLamp } from '../lib/beatLamp.svelte';

  const rt = $derived(app.s.runtime?.state);
  const disabled = $derived(app.s.connection !== 'connected' || app.s.permission === 'read_only');
  const beat = createBeatLamp();

  async function toggleMode() {
    if (!rt) return;
    if (rt.mode === 'Show') {
      await enterDesignMode(
        (command) => client.command(command),
        () => window.prompt('PIN for Design mode'),
      ).catch((e: Error) => notify(e.message, 'error'));
    } else {
      await run({ type: 'set_mode', mode: 'Show', pin: null });
    }
  }
</script>

<aside class="master" aria-label="Master controls">
  <button class="mode" class:show={rt?.mode === 'Show'} {disabled} onclick={toggleMode}>
    {rt?.mode === 'Show' ? 'SHOW mode' : 'Design mode'}
  </button>
  <button
    class="blackout"
    class:on={rt?.blackout}
    {disabled}
    onclick={() => run({ type: 'set_blackout', on: !rt?.blackout })}
  >
    {rt?.blackout ? 'BLACKOUT ON' : 'Blackout'}
  </button>
  <button class="tap" class:beat={beat.on} class:down={beat.on && beat.down} {disabled} onclick={() => run({ type: 'tap_tempo' })}>
    Tap
    <span class="bpm">{rt ? rt.tempo.bpm.toFixed(1) : '--'} BPM</span>
  </button>
  <button {disabled} onclick={() => run({ type: 'stop_all' })}>Stop all</button>
  <label class="gm">
    <span class="val">{Math.round((rt?.grand_master ?? 1) * 100)}%</span>
    <input
      class="fader"
      type="range"
      min="0"
      max="1"
      step="0.005"
      aria-label="Grand master"
      value={rt?.grand_master ?? 1}
      {disabled}
      oninput={(e) => run({ type: 'set_grand_master', level: Number(e.currentTarget.value) })}
    />
    <span class="title">GM</span>
  </label>
</aside>

<style>
  .master { display: flex; flex-direction: column; gap: 8px; padding: 10px 8px; width: 112px; background: var(--panel); border-left: 1px solid var(--border); min-height: 0; }
  .master button { width: 100%; padding: 10px 4px; font-weight: 600; }
  /* Room for two lines: their text wraps in some states and the bar stays still. */
  .master .mode, .master .blackout { flex: none; height: 60px; }
  .blackout.on { background: var(--danger); border-color: var(--danger); color: #fff; }
  .mode.show { background: var(--warn); color: #000; border-color: var(--warn); font-weight: 800; }
  .tap { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .tap.beat { background: color-mix(in srgb, var(--accent) 60%, var(--panel-2)); border-color: var(--accent); }
  .tap.beat.down { background: var(--accent); color: #000; }
  .tap.beat .bpm { color: inherit; }
  .bpm { font-size: 11px; font-weight: 400; color: var(--muted); font-variant-numeric: tabular-nums; }
  .gm { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 6px; min-height: 160px; border-top: 1px solid var(--border); padding-top: 8px; }
  .gm input { flex: 1; width: 56px; min-height: 120px; --cap-h: 28px; }
  .val { font-variant-numeric: tabular-nums; font-weight: 700; }
  .title { font-weight: 800; color: var(--muted); }
</style>
