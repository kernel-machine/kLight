<script lang="ts">
  // One time of a chaser step: the chaser's (shown as "= 1 beat") or its own,
  // picked from the core's list of beat lengths.
  import { beatChoices, formatBeats, formatTime, timeChoiceIndex } from '../lib/show';
  import type { TimeValue } from '../lib/protocol';

  let {
    label,
    own,
    fallback,
    zero = false,
    onchange,
  }: {
    /** Accessible name, e.g. "Hold of step 2". */
    label: string;
    /** The step's own time, or null: it follows the chaser. */
    own: TimeValue | null;
    /** The chaser's time, shown when the step has none of its own. */
    fallback: TimeValue;
    /** Offers no time at all, as for a fade. */
    zero?: boolean;
    onchange: (t: TimeValue | null) => void;
  } = $props();

  const beats = beatChoices();
  const choices = $derived(zero ? [0, ...beats] : beats);
  /** -2: as the chaser; -1: a time outside the list. */
  const current = $derived(own === null ? -2 : timeChoiceIndex(choices, own));

  function pick(index: number) {
    if (index === -2) return onchange(null);
    const b = choices[index];
    if (b !== undefined) onchange(b === 0 ? { ms: 0 } : { beats: b });
  }
  const show = (t: TimeValue) => ('ms' in t && t.ms === 0 ? '0' : formatTime(t));
</script>

<select class:own={own !== null} aria-label={label} value={current} title={own === null ? 'As the chaser' : 'This step only'} onchange={(e) => pick(Number(e.currentTarget.value))}>
  <option value={-2}>= {show(fallback)}</option>
  {#if current === -1 && own}<option value={-1} disabled>{formatTime(own)}</option>{/if}
  {#each choices as b, i (b)}<option value={i}>{b === 0 ? '0' : formatBeats(b)}</option>{/each}
</select>

<style>
  select { border-radius: 999px; padding: 3px 10px; font-size: 13px; color: var(--muted); background: transparent; }
  select.own { color: var(--text); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 16%, transparent); }
</style>
