<script lang="ts">
  // Collection contents: the scenes and effects it plays all together.
  import { app, notify, run } from '../lib/app.svelte';
  import { editQueue } from '../lib/queue';
  import { FUNCTION_CATEGORIES, collectionMemberTypes } from '../lib/show';
  import FunctionIcon from './FunctionIcon.svelte';
  import FunctionPickerDialog from './FunctionPickerDialog.svelte';
  import type { Collection, Function as Fn } from '../lib/protocol';

  let {
    f,
    collection,
    picking = $bindable(),
  }: {
    f: Fn;
    collection: Collection;
    /** Chaser or collection whose "add" dialog is open. */
    picking: number | null;
  } = $props();

  const show = $derived(app.s.show!);
  const playing = $derived(new Set(app.s.runtime?.state.active.find((a) => a.id === f.id)?.playing ?? []));
  const types = collectionMemberTypes();
  // Each function once.
  const canAdd = $derived(show.functions.some((x) => types.includes(x.kind.type) && !collection.functions.includes(x.id)));

  // Edits are queued and each one is applied to the latest version of the
  // collection: quick successive clicks must not overwrite each other.
  const enqueue = editQueue((e) => notify(String(e), 'error'));
  function update(change: (functions: number[]) => number[]) {
    const id = f.id;
    enqueue(() => {
      const current = app.s.show?.functions.find((x) => x.id === id);
      if (!current || current.kind.type !== 'collection') return;
      const functions = change(current.kind.functions);
      return run({ type: 'set_function', function: { ...current, kind: { ...current.kind, functions } } });
    });
  }

  function add(ids: number[]) {
    picking = null;
    update((functions) => [...functions, ...ids.filter((id) => !functions.includes(id))]);
  }

  const find = (id: number) => show.functions.find((x) => x.id === id);
  const typeLabel = (f: Fn) => FUNCTION_CATEGORIES.find((c) => c.id === f.kind.type)?.short;
</script>

<h3>Functions ({collection.functions.length})</h3>
<ul class="members" aria-label="Collection functions">
  {#each collection.functions as id, i (i)}
    {@const member = find(id)}
    <li class:current={playing.has(id)}>
      {#if member}<FunctionIcon f={member} />{/if}
      <span class="name">{#if member}{member.name}{:else}#{id} <span class="muted">(missing function)</span>{/if}</span>
      {#if member}<span class="muted type">{typeLabel(member)}</span>{/if}
      <button onclick={() => update((functions) => functions.filter((x) => x !== id))} aria-label="Remove {member?.name ?? `#${id}`}">×</button>
    </li>
  {:else}
    <li class="muted">Nothing yet: add the scenes and effects to play together.</li>
  {/each}
</ul>
<div class="row">
  <button class="primary" disabled={!canAdd} onclick={() => (picking = f.id)}>Add functions…</button>
  {#if !canAdd && !collection.functions.length}<span class="muted">Create a scene or an effect first.</span>{/if}
</div>
<p class="muted">All of them play together. Where several of their scenes drive the same channel, the highest value wins (HTP).</p>

{#if picking === f.id}
  <FunctionPickerDialog
    title="Add functions to {f.name}"
    only={types}
    exclude={collection.functions}
    empty="There is nothing more to add."
    confirmLabel={(n) => `Add ${n} function${n === 1 ? '' : 's'}`}
    onconfirm={add}
    onclose={() => (picking = null)}
  />
{/if}

<style>
  .members { margin: 0; padding: 0; list-style: none; border: 1px solid var(--border); border-radius: var(--radius); }
  li { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-bottom: 1px solid var(--border); }
  li:last-child { border-bottom: none; }
  .current { background: color-mix(in srgb, var(--accent) 20%, transparent); }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .type { font-size: 12px; }
</style>
