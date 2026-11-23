<script lang="ts">
  // Functions listed by type, each type with its own nestable folders.
  // Drag functions and folders onto a folder (or a type header) to move them.
  import { app, pref, run, setPref } from '../lib/app.svelte';
  import { nextId } from '../lib/show';
  import type { Function as Fn, FunctionCategory, FunctionFolder } from '../lib/protocol';

  let {
    selectedId,
    onselect,
    oncreate,
  }: {
    selectedId: number | null;
    onselect: (id: number) => void;
    oncreate: (category: FunctionCategory, folder: number | null) => void;
  } = $props();

  const show = $derived(app.s.show!);
  const active = $derived(new Map((app.s.runtime?.state.active ?? []).map((a) => [a.id, a])));

  const CATEGORIES: Array<{ id: FunctionCategory; label: string; one: string; short: string }> = [
    { id: 'scene', label: 'Scenes', one: 'scene', short: 'Scene' },
    { id: 'chaser', label: 'Chasers', one: 'chaser', short: 'Chaser' },
    { id: 'cue_list', label: 'Cue lists', one: 'cue list', short: 'Cue list' },
    { id: 'movement_effect', label: 'Movements', one: 'movement', short: 'Movement' },
    { id: 'matrix_effect', label: 'Matrix effects', one: 'matrix effect', short: 'Matrix' },
  ];
  /** Types with something in them (functions or folders): empty types are hidden. */
  const visibleCategories = $derived(
    CATEGORIES.filter((c) => show.functions.some((f) => f.kind.type === c.id) || show.function_folders.some((f) => f.category === c.id)),
  );

  // Collapsed types and folders are a local preference of this client.
  let collapsed = $state<Set<string>>(new Set(pref<string[]>('fx.collapsed', [])));
  $effect(() => setPref('fx.collapsed', [...collapsed]));
  function toggle(key: string) {
    const s = new Set(collapsed);
    if (s.has(key)) s.delete(key);
    else s.add(key);
    collapsed = s;
  }

  const byName = (a: { name: string }, b: { name: string }) =>
    a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' });
  const categoryOf = (f: Fn): FunctionCategory => f.kind.type;
  const foldersIn = (category: FunctionCategory, parent: number | null) =>
    show.function_folders.filter((f) => f.category === category && (f.parent ?? null) === parent).sort(byName);
  const functionsIn = (category: FunctionCategory, folder: number | null) =>
    show.functions.filter((f) => categoryOf(f) === category && (f.folder ?? null) === folder).sort(byName);

  /** Number of functions in a folder, subfolders included. */
  function countIn(folder: FunctionFolder): number {
    const sub = show.function_folders.filter((f) => f.parent === folder.id);
    return show.functions.filter((f) => f.folder === folder.id).length + sub.reduce((n, f) => n + countIn(f), 0);
  }

  async function newFolder(category: FunctionCategory, parent: number | null) {
    const name = window.prompt('Folder name', 'New folder');
    if (!name) return;
    const folder: FunctionFolder = { id: nextId(show.function_folders), category, name, parent };
    await run({ type: 'set_function_folder', folder });
    collapsed = new Set([...collapsed].filter((k) => k !== `c:${category}` && k !== `f:${parent}`));
  }

  function renameFolder(folder: FunctionFolder) {
    const name = window.prompt('Folder name', folder.name);
    if (name) run({ type: 'set_function_folder', folder: { ...folder, name } });
  }

  function deleteFolder(folder: FunctionFolder) {
    const n = countIn(folder);
    const msg = n ? `Delete folder "${folder.name}"? Its ${n} function${n === 1 ? '' : 's'} move up one level.` : `Delete folder "${folder.name}"?`;
    if (window.confirm(msg)) run({ type: 'remove_function_folder', id: folder.id });
  }

  // ---- drag & drop
  type Drag = { kind: 'function'; id: number; category: FunctionCategory } | { kind: 'folder'; id: number; category: FunctionCategory };
  let drag = $state<Drag | null>(null);
  let over = $state<string | null>(null);

  function canDrop(d: Drag | null, category: FunctionCategory, target: number | null): boolean {
    if (!d || d.category !== category) return false;
    if (d.kind === 'folder') {
      // Not into itself or one of its descendants.
      let cursor: number | null = target;
      while (cursor !== null) {
        if (cursor === d.id) return false;
        cursor = show.function_folders.find((f) => f.id === cursor)?.parent ?? null;
      }
    }
    return true;
  }

  function drop(category: FunctionCategory, target: number | null) {
    const d = drag;
    drag = null;
    over = null;
    if (!d || !canDrop(d, category, target)) return;
    if (d.kind === 'function') {
      const f = show.functions.find((x) => x.id === d.id);
      if (f && (f.folder ?? null) !== target) run({ type: 'set_function', function: { ...f, folder: target } });
    } else {
      const folder = show.function_folders.find((x) => x.id === d.id);
      if (folder && (folder.parent ?? null) !== target) run({ type: 'set_function_folder', folder: { ...folder, parent: target } });
    }
  }

  function allow(e: DragEvent, category: FunctionCategory, target: number | null) {
    if (canDrop(drag, category, target)) {
      e.preventDefault();
      over = `${category}:${target}`;
    }
  }

  function leave(category: FunctionCategory, target: number | null) {
    if (over === `${category}:${target}`) over = null;
  }
</script>

{#snippet functionRow(f: Fn, depth: number)}
  <div
    class="row-item fn"
    class:sel={f.id === selectedId}
    style="--depth:{depth}"
    draggable="true"
    role="treeitem"
    aria-selected={f.id === selectedId}
    tabindex="0"
    ondragstart={(e) => { drag = { kind: 'function', id: f.id, category: categoryOf(f) }; e.dataTransfer?.setData('text/plain', `f${f.id}`); }}
    ondragend={() => { drag = null; over = null; }}
    onclick={() => onselect(f.id)}
    onkeydown={(e) => { if (e.key === 'Enter') onselect(f.id); }}
  >
    <span class="dot" class:on={active.has(f.id)}></span>
    <span class="name">{f.name}</span>
    {#if active.has(f.id)}
      <button class="primary small" onclick={(e) => { e.stopPropagation(); run({ type: 'stop_function', id: f.id }); }}>Stop</button>
    {:else}
      <button class="small" onclick={(e) => { e.stopPropagation(); run({ type: 'start_function', id: f.id }); }}>Start</button>
    {/if}
  </div>
{/snippet}

{#snippet folderNode(folder: FunctionFolder, depth: number)}
  {@const key = `f:${folder.id}`}
  {@const open = !collapsed.has(key)}
  <div
    class="row-item folder"
    class:over={over === `${folder.category}:${folder.id}`}
    style="--depth:{depth}"
    draggable="true"
    role="treeitem"
    aria-expanded={open}
    aria-selected="false"
    aria-label="Folder {folder.name}"
    tabindex="0"
    ondragstart={(e) => { e.stopPropagation(); drag = { kind: 'folder', id: folder.id, category: folder.category }; e.dataTransfer?.setData('text/plain', `d${folder.id}`); }}
    ondragend={() => { drag = null; over = null; }}
    onclick={() => toggle(key)}
    onkeydown={(e) => { if (e.key === 'Enter') toggle(key); }}
    ondblclick={() => renameFolder(folder)}
    ondragover={(e) => allow(e, folder.category, folder.id)}
    ondragleave={() => leave(folder.category, folder.id)}
    ondrop={(e) => { e.preventDefault(); e.stopPropagation(); drop(folder.category, folder.id); }}
  >
    <span class="caret">{open ? '▾' : '▸'}</span>
    <span class="icon">📁</span>
    <span class="name">{folder.name}</span>
    <span class="muted count">{countIn(folder)}</span>
    <span class="actions">
      <button class="small" title="New function here" aria-label="New function in {folder.name}" onclick={(e) => { e.stopPropagation(); oncreate(folder.category, folder.id); }}>+</button>
      <button class="small" title="New subfolder" aria-label="New subfolder in {folder.name}" onclick={(e) => { e.stopPropagation(); newFolder(folder.category, folder.id); }}>📁+</button>
      <button class="small" title="Rename" aria-label="Rename {folder.name}" onclick={(e) => { e.stopPropagation(); renameFolder(folder); }}>✎</button>
      <button class="small" title="Delete folder" aria-label="Delete folder {folder.name}" onclick={(e) => { e.stopPropagation(); deleteFolder(folder); }}>×</button>
    </span>
  </div>
  {#if open}
    {#each foldersIn(folder.category, folder.id) as sub (sub.id)}
      {@render folderNode(sub, depth + 1)}
    {/each}
    {#each functionsIn(folder.category, folder.id) as f (f.id)}
      {@render functionRow(f, depth + 1)}
    {/each}
  {/if}
{/snippet}

<div class="toolbar">
  {#each CATEGORIES as c (c.id)}
    <button class="small" aria-label="New {c.one}" onclick={() => oncreate(c.id, null)}>+ {c.short}</button>
  {/each}
  <select class="small" aria-label="New folder in" value="" onchange={(e) => { const v = e.currentTarget.value as FunctionCategory; e.currentTarget.value = ''; if (v) newFolder(v, null); }}>
    <option value="">📁 New folder…</option>
    {#each CATEGORIES as c (c.id)}<option value={c.id}>in {c.label}</option>{/each}
  </select>
</div>

<div class="tree" role="tree">
  {#if !visibleCategories.length}
    <p class="muted empty-all">Nothing yet: create a scene, chaser, cue list or effect above.</p>
  {/if}
  {#each visibleCategories as c (c.id)}
    {@const key = `c:${c.id}`}
    {@const open = !collapsed.has(key)}
    {@const total = show.functions.filter((f) => categoryOf(f) === c.id).length}
    <div class="category">
      <div
        class="row-item head"
        class:over={over === `${c.id}:null`}
        role="treeitem"
        aria-expanded={open}
        aria-selected="false"
        tabindex="0"
        onclick={() => toggle(key)}
        onkeydown={(e) => { if (e.key === 'Enter') toggle(key); }}
        ondragover={(e) => allow(e, c.id, null)}
        ondragleave={() => leave(c.id, null)}
        ondrop={(e) => { e.preventDefault(); e.stopPropagation(); drop(c.id, null); }}
      >
        <span class="caret">{open ? '▾' : '▸'}</span>
        <b class="name">{c.label}</b>
        <span class="muted count">{total}</span>
        <span class="actions">
          <button class="small" aria-label="New {c.one} folder" title="New folder" onclick={(e) => { e.stopPropagation(); newFolder(c.id, null); }}>📁+</button>
        </span>
      </div>
      {#if open}
        {#each foldersIn(c.id, null) as folder (folder.id)}
          {@render folderNode(folder, 1)}
        {/each}
        {#each functionsIn(c.id, null) as f (f.id)}
          {@render functionRow(f, 1)}
        {/each}
      {/if}
    </div>
  {/each}
</div>

<style>
  .tree { display: flex; flex-direction: column; gap: 6px; }
  .toolbar { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 8px; }
  .empty-all { margin: 4px 0; font-size: 13px; }
  .category { border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .row-item { display: flex; align-items: center; gap: 6px; padding: 4px 6px 4px calc(6px + var(--depth, 0) * 16px); border-top: 1px solid var(--border); cursor: pointer; user-select: none; }
  .head { background: var(--panel-2); border-top: none; }
  .row-item.over { outline: 2px dashed var(--accent); outline-offset: -2px; }
  .fn.sel { background: color-mix(in srgb, var(--accent) 20%, transparent); }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .caret { width: 14px; color: var(--muted); }
  .count { font-size: 12px; }
  .actions { display: flex; gap: 3px; }
  .folder .actions { opacity: 0; }
  .folder:hover .actions, .folder:focus-within .actions { opacity: 1; }
  .small { padding: 2px 8px; font-size: 12px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--border); }
  .dot.on { background: var(--ok); box-shadow: 0 0 6px var(--ok); }
</style>
