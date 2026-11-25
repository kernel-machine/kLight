<script lang="ts">
  // A time that follows the tempo, picked from the core's list of beat lengths.
  // A value outside the list (milliseconds in an older show) is shown as it is
  // until another one is picked.
  import { beatChoices, formatBeats, formatTime, timeChoiceIndex } from '../lib/show';
  import type { TimeValue } from '../lib/protocol';

  let {
    label,
    value,
    zero = false,
    onchange,
  }: {
    label: string;
    value: TimeValue;
    /** Offers no time at all, as for a fade. */
    zero?: boolean;
    onchange: (t: TimeValue) => void;
  } = $props();

  const beats = beatChoices();
  const choices = $derived(zero ? [0, ...beats] : beats);
  const current = $derived(timeChoiceIndex(choices, value));

  function pick(index: number) {
    const b = choices[index];
    if (b !== undefined) onchange(b === 0 ? { ms: 0 } : { beats: b });
  }
</script>

<label class="row">
  {label}
  <select aria-label={label} value={current} onchange={(e) => pick(Number(e.currentTarget.value))}>
    {#if current < 0}<option value={-1} disabled>{formatTime(value)}</option>{/if}
    {#each choices as b, i (b)}<option value={i}>{b === 0 ? '0' : formatBeats(b)}</option>{/each}
  </select>
</label>
