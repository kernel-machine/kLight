<script lang="ts">
  // Dialog: pick the lights to add to a scene, from the same list as the
  // fixtures screen (universes, in each the groups with their fixtures, then
  // the fixtures in no group). A ticked group is added as a group.
  import { app } from '../lib/app.svelte';
  import { footprint, groupColor, groupRows, groupsOf } from '../lib/show';
  import type { PatchedFixture, Target } from '../lib/protocol';

  let {
    added,
    onconfirm,
    onclose,
  }: {
    /** What the scene has already: its groups and every fixture of them. */
    added: { groups: number[]; fixtures: number[] };
    onconfirm: (lights: Target[]) => void;
    onclose: () => void;
  } = $props();

  const show = $derived(app.s.show!);
  let groups = $state<number[]>([]);
  let fixtures = $state<number[]>([]);
  let collapsed = $state<Set<string>>(new Set());

  const hadGroup = $derived(new Set(added.groups));
  const had = $derived(new Set(added.fixtures));
  /** Fixtures coming in with a ticked group. */
  const grouped = $derived(
    new Set(show.groups.filter((g) => groups.includes(g.id)).flatMap((g) => g.members.map((m) => m.fixture))),
  );
  // A fixture ticked on its own and then covered by a group counts once.
  const alone = $derived(fixtures.filter((id) => !grouped.has(id)));
  const count = $derived(groups.length + alone.length);

  const fixturesOf = (universe: number) => show.fixtures.filter((f) => f.universe === universe).sort((a, b) => a.address - b.address);
  const groupsIn = (universe: number) =>
    show.groups
      .map((g) => ({ group: g, rows: [...new Map(groupRows(show, g, universe).map((r) => [r.fixture.id, r.fixture])).values()] }))
      .filter((x) => x.rows.length > 0);
  const ungrouped = (universe: number) => fixturesOf(universe).filter((f) => groupsOf(show, f.id).length === 0);

  function toggleOpen(key: string) {
    const s = new Set(collapsed);
    if (s.has(key)) s.delete(key);
    else s.add(key);
    collapsed = s;
  }
  const toggle = (list: number[], id: number) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  function confirm() {
    onconfirm([
      ...groups.map((id) => ({ type: 'group', id }) as Target),
      ...alone.map((id) => ({ type: 'fixture', id }) as Target),
    ]);
  }
</script>

{#snippet fixtureRow(f: PatchedFixture, depth: number)}
  {@const taken = had.has(f.id)}
  <label class="item" class:taken style="--depth:{depth}">
    <input
      type="checkbox"
      checked={taken || grouped.has(f.id) || fixtures.includes(f.id)}
      disabled={taken || grouped.has(f.id)}
      onchange={() => (fixtures = toggle(fixtures, f.id))}
      aria-label="Select {f.name}"
    />
    <span class="addr">{footprint(show, f) > 1 ? `${f.address}–${f.address + footprint(show, f) - 1}` : f.address}</span>
    <span class="name">{f.name}</span>
    {#if taken}<span class="muted">in the scene</span>{/if}
  </label>
{/snippet}

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onpointerdown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog panel" role="dialog" aria-modal="true" aria-label="Add lights">
    <h2>Add lights</h2>

    <div class="tree">
      {#each show.universes as u (u.id)}
        {@const key = `u:${u.id}`}
        {@const open = !collapsed.has(key)}
        <div class="universe">
          <button class="item head" aria-expanded={open} onclick={() => toggleOpen(key)}>
            <span class="caret">{open ? '▾' : '▸'}</span>
            <b class="name">{u.id}: {u.name}</b>
            <span class="muted">{fixturesOf(u.id).length} fixtures</span>
          </button>
          {#if open}
            {#each groupsIn(u.id) as { group: g, rows } (g.id)}
              {@const gkey = `g:${u.id}:${g.id}`}
              {@const gopen = !collapsed.has(gkey)}
              {@const taken = hadGroup.has(g.id)}
              <div class="item group" class:taken style="--gc:{groupColor(show, g.id)}">
                <button class="gcaret" aria-label={gopen ? `Collapse group ${g.name}` : `Expand group ${g.name}`} onclick={() => toggleOpen(gkey)}>{gopen ? '▾' : '▸'}</button>
                <label class="gname">
                  <input
                    type="checkbox"
                    checked={taken || groups.includes(g.id)}
                    disabled={taken}
                    onchange={() => (groups = toggle(groups, g.id))}
                    aria-label="Select group {g.name}"
                  />
                  <span class="swatch"></span>
                  <span class="name">{g.name}</span>
                </label>
                <span class="muted">{taken ? 'in the scene' : `group · ${rows.length}`}</span>
              </div>
              {#if gopen}
                {#each rows as f (f.id)}
                  {@render fixtureRow(f, 2)}
                {/each}
              {/if}
            {/each}
            {#each ungrouped(u.id) as f (f.id)}
              {@render fixtureRow(f, 1)}
            {/each}
          {/if}
        </div>
      {/each}
      {#if !show.fixtures.length}<p class="muted">No fixtures yet: patch them on the Fixtures screen.</p>{/if}
    </div>

    <div class="row end">
      <button onclick={onclose}>Cancel</button>
      <button class="primary" disabled={!count} onclick={confirm}>Add {count} light{count === 1 ? '' : 's'}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; z-index: 50; }
  .dialog { width: min(520px, 94vw); max-height: 88vh; display: flex; flex-direction: column; gap: 10px; }
  .tree { display: flex; flex-direction: column; gap: 6px; overflow: auto; min-height: 120px; }
  /* Never squeezed: a long list scrolls the tree. */
  .universe { flex: none; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  .item { display: flex; align-items: center; gap: 6px; width: 100%; padding: 4px 6px 4px calc(6px + var(--depth, 0) * 18px); border-top: 1px solid var(--border); cursor: pointer; user-select: none; }
  .head { background: var(--panel-2); border: none; border-radius: 0; text-align: left; }
  .group { background: var(--panel-2); border-left: 4px solid var(--gc); cursor: default; }
  .gcaret { border: none; background: transparent; width: 22px; padding: 0; }
  .gname { flex: 1; display: flex; align-items: center; gap: 6px; min-width: 0; font-weight: 600; cursor: pointer; }
  .swatch { width: 10px; height: 10px; border-radius: 50%; background: var(--gc); }
  .taken { opacity: 0.6; cursor: default; }
  .taken .gname { cursor: default; }
  .caret { width: 14px; color: var(--muted); }
  .addr { font-variant-numeric: tabular-nums; color: var(--muted); width: 64px; font-size: 12px; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .end { justify-content: flex-end; }
</style>
