<script lang="ts">
  // One list: universes, and in each universe the groups (as folders holding
  // their fixtures or heads) followed by the fixtures that belong to no group.
  // Click a group to edit it; drag fixtures onto a group to add them.
  import { app, run } from '../../lib/app.svelte';
  import type { FixtureGroup, PatchedFixture } from '../../lib/protocol';
  import { FIXTURE_DRAG, footprint, groupColor, groupRows, groupWithFixtures, groupsOf, groupsWithoutFixtures, nextId } from '../../lib/show';

  let {
    selected,
    focused,
    selectedGroup,
    ontoggle,
    onfocus,
    onselectgroup,
    oncreategroup,
  }: {
    selected: Set<number>;
    focused: number | null;
    selectedGroup: number | null;
    ontoggle: (id: number) => void;
    onfocus: (id: number) => void;
    onselectgroup: (id: number | null) => void;
    oncreategroup: () => void;
  } = $props();

  const show = $derived(app.s.show!);
  /** Collapsed universes (`u:1`) and groups inside a universe (`g:1:2`). */
  let collapsed = $state<Set<string>>(new Set());
  let over = $state<string | null>(null);

  function toggleOpen(key: string) {
    const s = new Set(collapsed);
    if (s.has(key)) s.delete(key);
    else s.add(key);
    collapsed = s;
  }

  function fixturesOf(universe: number) {
    return show.fixtures.filter((f) => f.universe === universe).sort((a, b) => a.address - b.address);
  }

  function used(universe: number) {
    return fixturesOf(universe).reduce((n, f) => n + footprint(show, f), 0);
  }

  /** Groups with something in this universe, and the fixtures in no group. */
  function groupsIn(universe: number) {
    return show.groups
      .map((g) => ({ group: g, rows: groupRows(show, g, universe) }))
      .filter((x) => x.rows.length > 0);
  }
  const ungrouped = (universe: number) => fixturesOf(universe).filter((f) => groupsOf(show, f.id).length === 0);
  const orphans = $derived(groupsWithoutFixtures(show));

  async function remove(id: number, name: string) {
    const n = fixturesOf(id).length;
    const msg = n ? `Delete "${name}" and its ${n} fixture${n === 1 ? '' : 's'}? (Undo is available)` : `Delete "${name}"?`;
    if (window.confirm(msg)) await run({ type: 'remove_universe', id });
  }

  function rename(id: number, current: string) {
    const name = window.prompt('Universe name', current);
    if (name) run({ type: 'rename_universe', id, name });
  }

  function dragStart(e: DragEvent, f: PatchedFixture) {
    // Dragging a ticked fixture drags the whole selection.
    const ids = selected.has(f.id) ? [...selected] : [f.id];
    e.dataTransfer?.setData(FIXTURE_DRAG, JSON.stringify(ids));
    e.dataTransfer?.setData('text/plain', ids.join(','));
  }

  function dropOn(e: DragEvent, g: FixtureGroup) {
    e.preventDefault();
    over = null;
    const raw = e.dataTransfer?.getData(FIXTURE_DRAG);
    if (!raw) return;
    let ids: unknown;
    try {
      ids = JSON.parse(raw);
    } catch {
      return;
    }
    if (!Array.isArray(ids)) return;
    const next = groupWithFixtures(show, g, ids.filter((x): x is number => typeof x === 'number'));
    if (next !== g) run({ type: 'set_group', group: next });
  }

  /** Takes a fixture (or one head of it) out of a group. */
  function removeFromGroup(g: FixtureGroup, fixture: number, head: number | null) {
    const members = g.members.filter((m) => !(m.fixture === fixture && (head === null || m.head === head)));
    run({ type: 'set_group', group: { ...g, members } });
  }

  /** Ticks or unticks every fixture of a group that is in this universe. */
  function toggleGroup(rows: ReturnType<typeof groupRows>) {
    const ids = [...new Set(rows.map((r) => r.fixture.id))];
    const all = ids.every((id) => selected.has(id));
    for (const id of ids) if (selected.has(id) === all) ontoggle(id);
  }
</script>

{#snippet fixtureRow(f: PatchedFixture, g: FixtureGroup | null, head: number | null)}
  {@const groups = groupsOf(show, f.id)}
  <li
    class:focused={focused === f.id}
    class:ingroup={g !== null}
    style="--gc:{g ? groupColor(show, g.id) : groups[0] ? groupColor(show, groups[0].id) : 'transparent'}"
    draggable="true"
    ondragstart={(e) => dragStart(e, f)}
  >
    <input type="checkbox" checked={selected.has(f.id)} onchange={() => ontoggle(f.id)} aria-label="Select {f.name}" />
    <button class="fx" onclick={() => onfocus(f.id)}>
      <span class="addr">{footprint(show, f) > 1 ? `${f.address}–${f.address + footprint(show, f) - 1}` : f.address}</span>
      <span class="name">{f.name}{head !== null ? ` · head ${head + 1}` : ''}</span>
      {#if g === null}
        <span class="dots">
          {#each groups as x (x.id)}<span class="dot" style="background:{groupColor(show, x.id)}" title={x.name}></span>{/each}
        </span>
      {/if}
    </button>
    {#if g}
      <button class="rm" title="Remove from group" aria-label="Remove {f.name}{head !== null ? ` head ${head + 1}` : ''} from {g.name}" onclick={() => removeFromGroup(g, f.id, head)}>×</button>
    {/if}
  </li>
{/snippet}

<div class="tree panel">
  <div class="row head">
    <h2>Universes</h2>
    <button onclick={() => run({ type: 'add_universe', universe: { id: nextId(show.universes), name: `Universe ${nextId(show.universes)}`, output: null } })}>+ Universe</button>
  </div>

  {#each show.universes as u (u.id)}
    {@const list = fixturesOf(u.id)}
    {@const open = !collapsed.has(`u:${u.id}`)}
    <div class="universe">
      <div class="uhead">
        <button class="caret" onclick={() => toggleOpen(`u:${u.id}`)} aria-label={open ? 'Collapse' : 'Expand'}>{open ? '▾' : '▸'}</button>
        <button class="uname" ondblclick={() => rename(u.id, u.name)} onclick={() => toggleOpen(`u:${u.id}`)} title="Double click to rename">
          <b>{u.id}: {u.name}</b>
          <span class="muted">{list.length} fixtures · {used(u.id)}/512 ch{u.output ? ` · ${u.output.kind}` : ''}</span>
        </button>
        <button class="del" title="Delete universe" aria-label="Delete universe {u.name}" onclick={() => remove(u.id, u.name)}>🗑</button>
      </div>
      {#if open}
        <ul>
          {#each groupsIn(u.id) as { group: g, rows } (g.id)}
            {@const key = `g:${u.id}:${g.id}`}
            {@const gopen = !collapsed.has(key)}
            {@const ticked = rows.length > 0 && rows.every((r) => selected.has(r.fixture.id))}
            <li
              class="ghead"
              class:sel={selectedGroup === g.id}
              class:over={over === key}
              style="--gc:{groupColor(show, g.id)}"
              role="treeitem"
              aria-selected={selectedGroup === g.id}
              aria-expanded={gopen}
              aria-label="Group {g.name}"
              tabindex="0"
              onclick={() => onselectgroup(selectedGroup === g.id ? null : g.id)}
              onkeydown={(e) => { if (e.key === 'Enter') onselectgroup(selectedGroup === g.id ? null : g.id); }}
              ondragover={(e) => { if (e.dataTransfer?.types.includes(FIXTURE_DRAG)) { e.preventDefault(); over = key; } }}
              ondragleave={() => { if (over === key) over = null; }}
              ondrop={(e) => dropOn(e, g)}
            >
              <button class="gcaret" aria-label={gopen ? 'Collapse group' : 'Expand group'} onclick={(e) => { e.stopPropagation(); toggleOpen(key); }}>{gopen ? '▾' : '▸'}</button>
              <input type="checkbox" checked={ticked} onclick={(e) => e.stopPropagation()} onchange={() => toggleGroup(rows)} aria-label="Select all in {g.name}" />
              <span class="swatch"></span>
              <span class="gname">{g.name}</span>
              <span class="muted count">{rows.length}</span>
            </li>
            {#if gopen}
              {#each rows as r (`${r.fixture.id}:${r.head}`)}
                {@render fixtureRow(r.fixture, g, r.head)}
              {/each}
            {/if}
          {/each}
          {#each ungrouped(u.id) as f (f.id)}
            {@render fixtureRow(f, null, null)}
          {:else}
            {#if groupsIn(u.id).length === 0}<li class="muted empty">No fixtures</li>{/if}
          {/each}
        </ul>
      {/if}
    </div>
  {/each}

  {#if orphans.length}
    <!-- Groups with no fixture in any universe: still listed, so they can be
         edited (drop fixtures on them) or deleted. -->
    <div class="universe">
      <div class="uhead"><span class="uname"><b>Groups without fixtures</b></span></div>
      <ul>
        {#each orphans as g (g.id)}
          {@const key = `g:none:${g.id}`}
          <li
            class="ghead"
            class:sel={selectedGroup === g.id}
            class:over={over === key}
            style="--gc:{groupColor(show, g.id)}"
            role="treeitem"
            aria-selected={selectedGroup === g.id}
            aria-label="Group {g.name}"
            tabindex="0"
            onclick={() => onselectgroup(selectedGroup === g.id ? null : g.id)}
            onkeydown={(e) => { if (e.key === 'Enter') onselectgroup(selectedGroup === g.id ? null : g.id); }}
            ondragover={(e) => { if (e.dataTransfer?.types.includes(FIXTURE_DRAG)) { e.preventDefault(); over = key; } }}
            ondragleave={() => { if (over === key) over = null; }}
            ondrop={(e) => dropOn(e, g)}
          >
            <span class="gcaret"></span>
            <span class="swatch"></span>
            <span class="gname">{g.name}</span>
            <span class="muted count">{g.members.length ? 'fixtures deleted' : 'empty'}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="footer row">
    <span class="muted">{selected.size} selected</span>
    <button class="primary" disabled={!selected.size} onclick={oncreategroup}>Create group</button>
  </div>
</div>

<style>
  .tree { display: flex; flex-direction: column; gap: 6px; }
  .head { justify-content: space-between; }
  .universe { border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .uhead { display: flex; align-items: stretch; background: var(--panel-2); }
  .uhead button { border: none; border-radius: 0; background: transparent; }
  .caret { width: 32px; }
  .uname { flex: 1; text-align: left; display: flex; flex-direction: column; gap: 2px; }
  .uname span { font-size: 12px; }
  .del { opacity: 0.7; }
  .del:hover { opacity: 1; color: var(--danger); }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: center; gap: 6px; padding: 2px 6px 2px 8px; border-top: 1px solid var(--border); border-left: 4px solid var(--gc); }
  li.ingroup { padding-left: 26px; }
  li.focused { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  li.empty { padding: 8px; border-left-color: transparent; }
  .ghead { background: var(--panel-2); cursor: pointer; user-select: none; padding-top: 4px; padding-bottom: 4px; }
  .ghead.sel { background: color-mix(in srgb, var(--gc) 28%, var(--panel-2)); }
  .ghead.over { outline: 2px dashed var(--accent); outline-offset: -2px; }
  .gcaret { border: none; background: transparent; width: 22px; padding: 0; }
  .swatch { width: 10px; height: 10px; border-radius: 50%; background: var(--gc); }
  .gname { flex: 1; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { font-size: 12px; }
  .fx { flex: 1; display: flex; gap: 8px; align-items: center; border: none; background: transparent; text-align: left; padding: 4px; }
  .addr { font-variant-numeric: tabular-nums; color: var(--muted); width: 64px; font-size: 12px; }
  .name { flex: 1; }
  .dots { display: flex; gap: 3px; }
  .dot { width: 10px; height: 10px; border-radius: 50%; }
  .rm { border: none; background: transparent; opacity: 0.6; }
  .rm:hover { opacity: 1; color: var(--danger); }
  .footer { justify-content: space-between; margin-top: 4px; }
</style>
