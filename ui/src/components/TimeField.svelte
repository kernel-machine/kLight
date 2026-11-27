<script lang="ts">
  // A cue time typed as text: seconds by default ("2", "0.5 s"), or "300 ms",
  // or "2 beats" to follow the tempo. What cannot be read is put back.
  import { formatSeconds, parseSeconds } from '../lib/show';
  import type { TimeValue } from '../lib/protocol';

  let {
    label,
    value,
    ms,
    onchange,
  }: {
    label: string;
    value: TimeValue;
    /** The same at the current tempo, shown on hover for beats. */
    ms?: number;
    onchange: (t: TimeValue) => void;
  } = $props();

  const title = $derived(
    'beats' in value && ms !== undefined ? `${label}: ${(ms / 1000).toLocaleString(undefined, { maximumFractionDigits: 2 })} s at this tempo` : label,
  );
</script>

<input
  class="time"
  aria-label={label}
  {title}
  value={formatSeconds(value)}
  onchange={(e) => {
    const input = e.currentTarget;
    const t = parseSeconds(input.value);
    if (t && JSON.stringify(t) !== JSON.stringify(value)) onchange(t);
    else input.value = formatSeconds(value);
  }}
  onkeydown={(e) => { if (e.key === 'Enter') e.currentTarget.blur(); }}
/>

<style>
  .time { width: 78px; }
</style>
