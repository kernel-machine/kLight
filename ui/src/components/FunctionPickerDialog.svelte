<script lang="ts">
  // Dialog: pick functions from the same tree as the FX screen (type > folders),
  // optionally ordering them; the caller decides what to do with them.
  import type { Snippet } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { FUNCTION_CATEGORIES, colorToHex, foldersIn, functionColor, functionsIn, functionsUnder } from '../lib/show';
  import type { Function as Fn, FunctionCategory, FunctionFolder } from '../lib/protocol';

  let {
    title,
    only,
    ordering,
    empty,
    confirmLabel,
    onconfirm,
    onclose,
    options,
  }: {
    title: string;
    /** Types of function offered (all when omitted). */
    only?: FunctionCategory[];
    /** Lists the chosen functions to order them here; `hint` shows while empty. */
    ordering?: { title: string; hint: string };
    /** Shown when there is nothing to choose from. */
    empty: string;
    confirmLabel: (count: number) => string;
    /** The chosen functions, in the order shown or else ticked. */
    onconfirm: (ids: number[]) => void;
    onclose: () => void;
    /** Extra settings, above the tree. */
    options?: Snippet;
  } = $props();

  const show = $derived(app.s.show!);
  /** Chosen functions, in order. */
  let order = $state<number[]>([]);
  let collapsed = $state<Set<string>>(new Set());

  const chosen = $derived(new Set(order));
  const categories = $derived(
    FUNCTION_CATEGORIES.filter((c) => (!only || only.includes(c.id)) && show.functions.some((f) => f.kind.type === c.id)),
  );
  const ordered = $derived(order.flatMap((id) => show.functions.filter((f) => f.id === id)));

  function toggleOpen(key: string) {
    const s = new Set(collapsed);
    if (s.has(key)) s.delete(key);
    else s.add(key);
    collapsed = s;
  }

  function toggleFunction(id: number) {
    order = chosen.has(id) ? order.filter((x) => x !== id) : [...order, id];
  }

  /** All / some / none of the functions of a group are chosen. */
  function stateOf(fns: Fn[]): 'all' | 'some' | 'none' {
    const n = fns.filter((f) => chosen.has(f.id)).length;
    return n === 0 ? 'none' : n === fns.length ? 'all' : 'some';
  }

  function toggleGroup(fns: Fn[]) {
    if (stateOf(fns) === 'all') {
      const ids = new Set(fns.map((f) => f.id));
      order = order.filter((id) => !ids.has(id));
    } else {
      order = [...order, ...fns.filter((f) => !chosen.has(f.id)).map((f) => f.id)];
    }
  }

  function move(i: number, d: -1 | 1) {
    const j = i + d;
    if (j < 0 || j >= order.length) return;
    const next = [...order];
    [next[i], next[j]] = [next[j]!, next[i]!];
    order = next;
  }

  function indeterminate(node: HTMLInputElement, on: boolean) {
    node.indeterminate = on;
    return { update: (v: boolean) => (node.indeterminate = v) };
  }
</script>

{#snippet groupBox(fns: Fn[], label: string)}
  {@const st = stateOf(fns)}
  <input
    type="checkbox"
    checked={st === 'all'}
    use:indeterminate={st === 'some'}
    disabled={!fns.length}
    onclick={(e) => e.stopPropagation()}
    onchange={() => toggleGroup(fns)}
    aria-label="Select all in {label}"
  />
{/snippet}

{#snippet functionRow(f: Fn, depth: number)}
  {@const c = functionColor(show, f)}
  <label class="item fn" style="--depth:{depth}">
    <input type="checkbox" checked={chosen.has(f.id)} onchange={() => toggleFunction(f.id)} aria-label="Select {f.name}" />
    <span class="swatch" style={c ? `background:${colorToHex(c)}` : ''}></span>
    <span class="name">{f.name}</span>
  </label>
{/snippet}

{#snippet folderNode(folder: FunctionFolder, depth: number)}
  {@const key = `f:${folder.id}`}
  {@const open = !collapsed.has(key)}
  <div
    class="item folder"
    style="--depth:{depth}"
    role="treeitem"
    aria-selected="false"
    aria-expanded={open}
    aria-label="Folder {folder.name}"
    tabindex="0"
    onclick={() => toggleOpen(key)}
    onkeydown={(e) => e.key === 'Enter' && toggleOpen(key)}
  >
    <span class="caret">{open ? '▾' : '▸'}</span>
    {@render groupBox(functionsUnder(show, folder.category, folder.id), folder.name)}
    <span class="icon">📁</span>
    <span class="name">{folder.name}</span>
  </div>
  {#if open}
    {#each foldersIn(show, folder.category, folder.id) as sub (sub.id)}
      {@render folderNode(sub, depth + 1)}
    {/each}
    {#each functionsIn(show, folder.category, folder.id) as f (f.id)}
      {@render functionRow(f, depth + 1)}
    {/each}
  {/if}
{/snippet}

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog panel" role="dialog" aria-modal="true" aria-label={title}>
    <h2>{title}</h2>
    {@render options?.()}

    <div class="tree" role="tree">
      {#each categories as c (c.id)}
        {@const key = `c:${c.id}`}
        {@const open = !collapsed.has(key)}
        <div class="category">
          <div
            class="item head"
            role="treeitem"
            aria-selected="false"
            aria-expanded={open}
            tabindex="0"
            onclick={() => toggleOpen(key)}
            onkeydown={(e) => e.key === 'Enter' && toggleOpen(key)}
          >
            <span class="caret">{open ? '▾' : '▸'}</span>
            {@render groupBox(functionsUnder(show, c.id, null), c.label)}
            <b class="name">{c.label}</b>
          </div>
          {#if open}
            {#each foldersIn(show, c.id, null) as folder (folder.id)}
              {@render folderNode(folder, 1)}
            {/each}
            {#each functionsIn(show, c.id, null) as f (f.id)}
              {@render functionRow(f, 1)}
            {/each}
          {/if}
        </div>
      {:else}
        <p class="muted">{empty}</p>
      {/each}
    </div>

    {#if ordering}
      <h3>{ordering.title} <span class="muted">({ordered.length})</span></h3>
      <ol class="order">
        {#each ordered as f, i (f.id)}
          <li>
            <span class="name">{f.name}</span>
            <button disabled={i === 0} onclick={() => move(i, -1)} aria-label="Move {f.name} up">↑</button>
            <button disabled={i === ordered.length - 1} onclick={() => move(i, 1)} aria-label="Move {f.name} down">↓</button>
            <button onclick={() => toggleFunction(f.id)} aria-label="Remove {f.name}">×</button>
          </li>
        {:else}
          <li class="muted">{ordering.hint}</li>
        {/each}
      </ol>
    {/if}

    <div class="row end">
      <button onclick={onclose}>Cancel</button>
      <button class="primary" disabled={!ordered.length} onclick={() => onconfirm(ordered.map((f) => f.id))}>{confirmLabel(ordered.length)}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; z-index: 50; }
  .dialog { width: min(520px, 94vw); max-height: 88vh; display: flex; flex-direction: column; gap: 10px; overflow: auto; }
  .tree { display: flex; flex-direction: column; gap: 6px; overflow: auto; min-height: 120px; max-height: 40vh; }
  .category { border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .item { display: flex; align-items: center; gap: 6px; padding: 4px 6px 4px calc(6px + var(--depth, 0) * 16px); border-top: 1px solid var(--border); cursor: pointer; user-select: none; }
  .head { background: var(--panel-2); border-top: none; }
  .caret { width: 14px; color: var(--muted); }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .swatch { width: 14px; height: 14px; border-radius: 4px; border: 1px solid var(--border); background: transparent; }
  .order { margin: 0; padding: 0; list-style: none; border: 1px solid var(--border); border-radius: var(--radius); max-height: 24vh; overflow: auto; }
  .order li { display: flex; align-items: center; gap: 6px; padding: 3px 8px; border-bottom: 1px solid var(--border); }
  .order li:last-child { border-bottom: none; }
  .end { justify-content: flex-end; }
</style>
