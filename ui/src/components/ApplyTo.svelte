<script lang="ts">
  // What the next values of a scene go to: all its lights, one of its groups,
  // or some fixtures. Groups come first, each in its color; the fixtures are in
  // a box per model, headed by a bar that chooses them all, with a dot for
  // every group they belong to. It is also the list of the scene's lights:
  // each group, and each fixture added on its own, has a × that takes it out
  // (a fixture there through a group goes with the group).
  import { app } from '../lib/app.svelte';
  import { groupColor, groupsOf } from '../lib/show';
  import { found, lightSections } from '../lib/lightSections';
  import type { FixtureGroup, SceneLights, Target } from '../lib/protocol';

  let {
    lights,
    groups,
    group,
    chosen,
    onall,
    ongroup,
    onfixtures,
    onremove,
  }: {
    lights: SceneLights;
    /** The groups of the scene. */
    groups: FixtureGroup[];
    /** The chosen group, if any. */
    group: number | null;
    /** The chosen fixtures (none while a group or all lights are). */
    chosen: number[];
    onall: () => void;
    ongroup: (id: number) => void;
    onfixtures: (ids: number[]) => void;
    /** Takes a light (a group, or a fixture added on its own) out of the scene. */
    onremove: (light: Target) => void;
  } = $props();

  const show = $derived(app.s.show!);
  let query = $state('');
  const all = $derived(group === null && !chosen.length);
  const shownGroups = $derived(groups.filter((g) => found(g.name, query)));
  const sections = $derived(lightSections(show.fixtures.filter((f) => lights.fixtures.includes(f.id)), query));
  const countOf = (id: number) => lights.members.find((m) => m.target.type === 'group' && m.target.id === id)?.fixtures.length ?? 0;
  const groupName = $derived(groups.find((g) => g.id === group)?.name);
  /** Fixtures that are lights of the scene on their own, not only through a group. */
  const own = $derived(new Set(lights.members.flatMap((m) => (m.target.type === 'fixture' ? [m.target.id] : []))));
  /** The groups of the scene a fixture is in it through. */
  const through = (id: number) =>
    lights.members.flatMap((m) => (m.target.type === 'group' && m.fixtures.includes(id) ? groups.filter((g) => g.id === (m.target as { id: number }).id).map((g) => g.name) : []));

  function toggle(id: number) {
    onfixtures(chosen.includes(id) ? chosen.filter((x) => x !== id) : [...chosen, id]);
  }
  /** Chooses the fixtures of a section, or lets them all go if they are. */
  function toggleSection(ids: number[]) {
    const had = ids.every((id) => chosen.includes(id));
    onfixtures(had ? chosen.filter((id) => !ids.includes(id)) : [...chosen, ...ids.filter((id) => !chosen.includes(id))]);
  }
</script>

<div class="apply" role="group" aria-label="Apply to">
  <div class="row">
    <button class="chip" class:selected={all} aria-pressed={all} aria-label="All lights" onclick={onall}>All lights <small>{lights.fixtures.length}</small></button>
    <!-- Always told: a chosen light may be hidden by the search or the scroll. -->
    <span class="chosen">
      {#if group !== null}Group {groupName}: {countOf(group)} fixture{countOf(group) === 1 ? '' : 's'}
      {:else if chosen.length}{chosen.length} fixture{chosen.length === 1 ? '' : 's'} chosen{/if}
    </span>
    <input type="search" placeholder="Find a group or fixture…" aria-label="Find a light" bind:value={query} />
  </div>

  <div class="lists">
    {#if shownGroups.length}
      <h4>Groups</h4>
      <div class="groups">
        {#each shownGroups as g (g.id)}
          <span class="item group-item" class:on={group === g.id} style="--gc:{groupColor(show, g.id)}">
            <button class="group" class:on={group === g.id} aria-pressed={group === g.id} aria-label="Group: {g.name}" title={lights.members.find((m) => m.target.type === 'group' && m.target.id === g.id)?.fixtures.map((id) => show.fixtures.find((f) => f.id === id)?.name).join(', ')} onclick={() => ongroup(g.id)}>
              <span class="dot"></span>{g.name} <small>{countOf(g.id)}</small>
            </button>
            <button class="x" aria-label="Remove {g.name} from the scene" title="Remove the group from the scene" onclick={() => onremove({ type: 'group', id: g.id })}>×</button>
          </span>
        {/each}
      </div>
    {/if}
    {#if sections.length}
      <h4>Fixtures</h4>
      {#each sections as s (s.key)}
        {@const ids = s.fixtures.map((f) => f.id)}
        {@const picked = ids.filter((id) => chosen.includes(id)).length}
        {@const whole = picked === ids.length}
        <div class="section">
          <button class="model" class:on={whole} class:some={picked > 0 && !whole} aria-pressed={whole ? true : picked ? 'mixed' : false} aria-label="All {s.title}" onclick={() => toggleSection(ids)}>
            <span class="box" aria-hidden="true">{whole ? '✓' : picked ? '–' : ''}</span>
            <span class="name">{s.title}</span>
            <small>{picked && !whole ? `${picked} of ${ids.length}` : ids.length}</small>
            <span class="hint">{whole ? 'Clear' : 'Select all'}</span>
          </button>
          <div class="fixtures">
            {#each s.fixtures as f (f.id)}
              {@const via = through(f.id)}
              <span class="item" class:on={chosen.includes(f.id)}>
                <button class="fixture" class:on={chosen.includes(f.id)} aria-pressed={chosen.includes(f.id)} aria-label={f.name} title={[f.name, ...groupsOf(show, f.id).map((g) => `group ${g.name}`)].join(' · ')} onclick={() => toggle(f.id)}>
                  <span class="name">{f.name}</span>
                  <span class="dots" aria-hidden="true">{#each groupsOf(show, f.id) as g (g.id)}<span class="dot" style="--gc:{groupColor(show, g.id)}"></span>{/each}</span>
                </button>
                {#if own.has(f.id)}
                  <button class="x" aria-label="Remove {f.name} from the scene" title="Remove from the scene" onclick={() => onremove({ type: 'fixture', id: f.id })}>×</button>
                {:else}
                  <span class="via" title="In the scene through {via.join(', ')}: remove the group to take it out">⛓</span>
                {/if}
              </span>
            {/each}
          </div>
        </div>
      {/each}
    {/if}
    {#if !shownGroups.length && !sections.length}<span class="muted">Nothing matches “{query}”.</span>{/if}
  </div>
</div>

<style>
  .apply { display: flex; flex-direction: column; gap: 8px; }
  .apply input { margin-left: auto; width: min(240px, 100%); }
  .chosen { font-size: 12px; color: var(--accent); font-weight: 600; }
  .lists { display: flex; flex-direction: column; gap: 6px; max-height: max(320px, 45vh); overflow: auto; }
  h4 { margin: 4px 0 0; font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); }
  small { opacity: 0.7; font-weight: 400; }
  .dot { display: inline-block; flex: none; width: 9px; height: 9px; border-radius: 50%; background: var(--gc); }
  .groups { display: flex; flex-wrap: wrap; gap: 6px; }
  /* A light and its ×: one control to the eye, two buttons. */
  .item { display: flex; align-items: stretch; min-width: 0; }
  .item > :first-child { flex: 1; min-width: 0; border-top-right-radius: 0; border-bottom-right-radius: 0; }
  .x, .via { flex: none; display: flex; align-items: center; justify-content: center; width: 26px; padding: 0; border-top-left-radius: 0; border-bottom-left-radius: 0; border-left: none; font-size: 14px; line-height: 1; color: var(--muted); }
  .x:hover { color: var(--danger, #e5484d); background: color-mix(in srgb, var(--danger, #e5484d) 14%, var(--panel)); }
  .via { border: 1px solid var(--border); border-left: none; border-radius: 0 var(--radius) var(--radius) 0; font-size: 11px; opacity: 0.7; cursor: help; }
  .group { display: flex; align-items: center; gap: 6px; font-weight: 600; padding: 5px 12px; border: 1px solid var(--gc); background: color-mix(in srgb, var(--gc) 16%, var(--panel)); }
  .group.on { background: var(--gc); color: #101318; }
  .group-item .x { border: 1px solid var(--gc); border-left: none; background: color-mix(in srgb, var(--gc) 16%, var(--panel)); }
  .item.on .x, .item.on .via { border-color: var(--accent); }
  .group-item.on .x { border-color: var(--gc); }
  .group.on .dot { background: #101318; }
  /* Never squeezed: a long list scrolls. */
  .section { flex: none; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
  /* A title bar, unlike the fixtures under it. */
  .model { display: flex; align-items: center; gap: 8px; width: 100%; padding: 5px 8px; border: none; border-radius: 0; font-size: 13px; font-weight: 700; text-align: left; background: color-mix(in srgb, var(--text) 10%, var(--panel-2)); }
  .model:hover { background: color-mix(in srgb, var(--accent) 22%, var(--panel-2)); }
  .box { display: inline-flex; flex: none; align-items: center; justify-content: center; width: 16px; height: 16px; border: 1px solid var(--muted); border-radius: 4px; background: var(--panel); font-size: 12px; line-height: 1; }
  .model.on .box, .model.some .box { background: var(--accent); border-color: var(--accent); color: #fff; }
  .hint { margin-left: auto; font-size: 11px; font-weight: 400; color: var(--muted); }
  .fixtures { display: grid; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); gap: 4px; padding: 6px; }
  .fixture { display: flex; justify-content: space-between; align-items: center; gap: 6px; padding: 4px 8px; font-size: 12px; text-align: left; min-width: 0; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .dots { display: flex; gap: 3px; }
  .fixture.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  /* Dots stay readable on a chosen fixture. */
  .fixture.on .dot { box-shadow: 0 0 0 1px #fff; }
</style>
