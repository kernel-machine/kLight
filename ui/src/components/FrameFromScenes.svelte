<script lang="ts">
  // Dialog: pick functions, order them, choose the frame type; the caller
  // builds the frame.
  import FunctionPickerDialog from './FunctionPickerDialog.svelte';

  let {
    oncreate,
    onclose,
  }: {
    oncreate: (opts: { ids: number[]; solo: boolean; name: string }) => void;
    onclose: () => void;
  } = $props();

  let solo = $state(false);
  let name = $state('Scenes');
</script>

<FunctionPickerDialog
  title="Frame from functions"
  ordering={{ title: 'Button order', hint: 'Tick functions above; they appear here in the order of the buttons.' }}
  empty="There are no functions yet."
  confirmLabel={(n) => `Create ${n} button${n === 1 ? '' : 's'}`}
  onconfirm={(ids) => oncreate({ ids, solo, name: name.trim() || 'Scenes' })}
  {onclose}
>
  {#snippet options()}
    <div class="row">
      <label class="row"><input type="radio" name="frame-type" checked={!solo} onchange={() => (solo = false)} /> Frame</label>
      <label class="row"><input type="radio" name="frame-type" checked={solo} onchange={() => (solo = true)} /> Solo frame (one at a time)</label>
    </div>
    <label class="col">Frame name <input bind:value={name} /></label>
  {/snippet}
</FunctionPickerDialog>

<style>
  .col { display: flex; flex-direction: column; gap: 4px; }
</style>
