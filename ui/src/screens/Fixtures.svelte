<script lang="ts">
  // Universes and their fixtures on the left; details, patching and groups on the right.
  import { app, run } from '../lib/app.svelte';
  import { headCount, nextId } from '../lib/show';
  import type { FixtureGroup } from '../lib/protocol';
  import UniverseTree from '../components/fixtures/UniverseTree.svelte';
  import FixtureDetails from '../components/fixtures/FixtureDetails.svelte';
  import AddFixtures from '../components/fixtures/AddFixtures.svelte';
  import GroupEditor from '../components/fixtures/GroupEditor.svelte';

  const show = $derived(app.s.show!);
  let selected = $state<Set<number>>(new Set());
  let focused = $state<number | null>(null);
  const focusedUniverse = $derived(show.fixtures.find((f) => f.id === focused)?.universe ?? show.universes[0]?.id ?? 1);
  const warnings = $derived((app.s.runtime?.warnings ?? []).filter((w) => w.includes('conflict') || w.includes('skipped')));

  // Forget fixtures that no longer exist (unpatched, universe removed).
  $effect(() => {
    const ids = new Set(show.fixtures.map((f) => f.id));
    if ([...selected].some((id) => !ids.has(id))) selected = new Set([...selected].filter((id) => ids.has(id)));
    if (focused !== null && !ids.has(focused)) focused = null;
  });

  function toggle(id: number) {
    const s = new Set(selected);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selected = s;
  }

  async function createGroup() {
    const fixtures = show.fixtures.filter((f) => selected.has(f.id)).sort((a, b) => a.universe - b.universe || a.address - b.address);
    if (!fixtures.length) return;
    const name = window.prompt('Group name', `Group ${nextId(show.groups)}`);
    if (!name) return;
    const heads = fixtures.flatMap((f) => Array.from({ length: headCount(show, f) }, (_, head) => ({ fixture: f.id, head })));
    const width = heads.length <= 8 ? heads.length : Math.ceil(Math.sqrt(heads.length));
    const group: FixtureGroup = {
      id: nextId(show.groups),
      name,
      members: heads.map((m, i) => ({ ...m, grid: { x: i % width, y: Math.floor(i / width) } })),
      grid_width: width,
      grid_height: Math.ceil(heads.length / width),
    };
    if (await run({ type: 'set_group', group })) selected = new Set();
  }
</script>

<div class="layout">
  <UniverseTree {selected} {focused} ontoggle={toggle} onfocus={(id) => (focused = id)} oncreategroup={createGroup} />
  <div class="col right">
    {#each warnings as w}<div class="badge warn">{w}</div>{/each}
    {#if focused !== null}
      <FixtureDetails id={focused} onclose={() => (focused = null)} />
    {/if}
    <AddFixtures defaultUniverse={focusedUniverse} />
    <GroupEditor {selected} />
  </div>
</div>

<style>
  .layout { display: grid; grid-template-columns: minmax(300px, 380px) 1fr; gap: 12px; align-items: start; }
  @media (max-width: 900px) { .layout { grid-template-columns: 1fr; } }
  .right { min-width: 0; }
</style>
