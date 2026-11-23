<script lang="ts">
  // Universes as an expandable list; each one shows its fixtures.
  import { app, run } from '../../lib/app.svelte';
  import { footprint, groupColor, groupsOf, nextId } from '../../lib/show';

  let {
    selected,
    focused,
    ontoggle,
    onfocus,
    oncreategroup,
  }: {
    selected: Set<number>;
    focused: number | null;
    ontoggle: (id: number) => void;
    onfocus: (id: number) => void;
    oncreategroup: () => void;
  } = $props();

  const show = $derived(app.s.show!);
  let collapsed = $state<Set<number>>(new Set());

  function toggleOpen(id: number) {
    const s = new Set(collapsed);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    collapsed = s;
  }

  function fixturesOf(universe: number) {
    return show.fixtures.filter((f) => f.universe === universe).sort((a, b) => a.address - b.address);
  }

  function used(universe: number) {
    return fixturesOf(universe).reduce((n, f) => n + footprint(show, f), 0);
  }

  async function remove(id: number, name: string) {
    const n = fixturesOf(id).length;
    const msg = n ? `Delete "${name}" and its ${n} fixture${n === 1 ? '' : 's'}? (Undo is available)` : `Delete "${name}"?`;
    if (window.confirm(msg)) await run({ type: 'remove_universe', id });
  }

  function rename(id: number, current: string) {
    const name = window.prompt('Universe name', current);
    if (name) run({ type: 'rename_universe', id, name });
  }
</script>

<div class="tree panel">
  <div class="row head">
    <h2>Universes</h2>
    <button onclick={() => run({ type: 'add_universe', universe: { id: nextId(show.universes), name: `Universe ${nextId(show.universes)}`, output: null } })}>+ Universe</button>
  </div>

  {#each show.universes as u (u.id)}
    {@const list = fixturesOf(u.id)}
    {@const open = !collapsed.has(u.id)}
    <div class="universe">
      <div class="uhead">
        <button class="caret" onclick={() => toggleOpen(u.id)} aria-label={open ? 'Collapse' : 'Expand'}>{open ? '▾' : '▸'}</button>
        <button class="uname" ondblclick={() => rename(u.id, u.name)} onclick={() => toggleOpen(u.id)} title="Double click to rename">
          <b>{u.id}: {u.name}</b>
          <span class="muted">{list.length} fixtures · {used(u.id)}/512 ch{u.output ? ` · ${u.output.kind}` : ''}</span>
        </button>
        <button class="del" title="Delete universe" aria-label="Delete universe {u.name}" onclick={() => remove(u.id, u.name)}>🗑</button>
      </div>
      {#if open}
        <ul>
          {#each list as f (f.id)}
            {@const groups = groupsOf(show, f.id)}
            <li class:focused={focused === f.id} style="--gc:{groups[0] ? groupColor(show, groups[0].id) : 'transparent'}">
              <input type="checkbox" checked={selected.has(f.id)} onchange={() => ontoggle(f.id)} aria-label="Select {f.name}" />
              <button class="fx" onclick={() => onfocus(f.id)}>
                <span class="addr">{footprint(show, f) > 1 ? `${f.address}–${f.address + footprint(show, f) - 1}` : f.address}</span>
                <span class="name">{f.name}</span>
                <span class="dots">
                  {#each groups as g (g.id)}<span class="dot" style="background:{groupColor(show, g.id)}" title={g.name}></span>{/each}
                </span>
              </button>
            </li>
          {:else}
            <li class="muted empty">No fixtures</li>
          {/each}
        </ul>
      {/if}
    </div>
  {/each}

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
  li.focused { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  li.empty { padding: 8px; border-left-color: transparent; }
  .fx { flex: 1; display: flex; gap: 8px; align-items: center; border: none; background: transparent; text-align: left; padding: 4px; }
  .addr { font-variant-numeric: tabular-nums; color: var(--muted); width: 64px; font-size: 12px; }
  .name { flex: 1; }
  .dots { display: flex; gap: 3px; }
  .dot { width: 10px; height: 10px; border-radius: 50%; }
  .footer { justify-content: space-between; margin-top: 4px; }
</style>
