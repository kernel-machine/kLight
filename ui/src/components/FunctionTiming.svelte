<script lang="ts">
  // The tempo settings of a function, outside its editor: step and fade of a
  // chaser, cycle of a movement or matrix effect. Nothing for other types.
  import { app, notify, run } from '../lib/app.svelte';
  import { editQueue } from '../lib/queue';
  import TimeSelect from './TimeSelect.svelte';
  import type { Function as Fn, TimeValue } from '../lib/protocol';

  let { f }: { f: Fn } = $props();

  // Each edit is applied to the latest version of the function: quick
  // successive changes must not overwrite each other.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function set(key: 'hold' | 'fade' | 'period', time: TimeValue) {
    const id = f.id;
    enqueue(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || !(key in current.kind)) return;
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, [key]: time } } });
    });
  }
</script>

{#if f.kind.type === 'chaser'}
  <TimeSelect label="Hold" value={f.kind.hold} onchange={(t) => set('hold', t)} />
  <TimeSelect label="Fade" zero value={f.kind.fade} onchange={(t) => set('fade', t)} />
{:else if f.kind.type === 'movement_effect' || f.kind.type === 'matrix_effect'}
  <TimeSelect label="Cycle" value={f.kind.period} onchange={(t) => set('period', t)} />
{/if}
