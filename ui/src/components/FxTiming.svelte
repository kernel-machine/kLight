<script lang="ts">
  // Timing card shared by the movement and matrix editors.
  import TimeSelect from './TimeSelect.svelte';
  import type { Direction, RunMode, TimeValue } from '../lib/protocol';

  type Timing = { period: TimeValue; run_mode: RunMode; direction: Direction; fade_in: TimeValue; fade_out: TimeValue };

  let { value, onchange }: { value: Timing; onchange: (patch: Partial<Timing>) => void } = $props();

  const RUN_MODES: Array<[RunMode, string]> = [['Loop', 'Loop'], ['SingleShot', 'Single shot'], ['PingPong', 'Ping-pong']];
  const DIRECTIONS: Direction[] = ['Forward', 'Backward'];
</script>

<section class="fx-card">
  <h3>Timing</h3>
  <div class="fx-timing">
    <TimeSelect label="Cycle" value={value.period} onchange={(period) => onchange({ period })} />
    <label class="row">Mode
      <select value={value.run_mode} aria-label="Run mode" onchange={(e) => onchange({ run_mode: e.currentTarget.value as RunMode })}>
        {#each RUN_MODES as [v, label] (v)}<option value={v}>{label}</option>{/each}
      </select>
    </label>
    <div class="fx-seg" role="group" aria-label="Direction">
      {#each DIRECTIONS as d (d)}
        <button class:active={value.direction === d} aria-pressed={value.direction === d} onclick={() => onchange({ direction: d })}>{d}</button>
      {/each}
    </div>
    <TimeSelect label="Fade in" zero value={value.fade_in} onchange={(fade_in) => onchange({ fade_in })} />
    <TimeSelect label="Fade out" zero value={value.fade_out} onchange={(fade_out) => onchange({ fade_out })} />
  </div>
</section>
