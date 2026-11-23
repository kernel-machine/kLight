<script lang="ts">
  // Groups: a matrix where heads are dragged to set position and order
  // (order = row by row). Click a head then a cell also works (touch).
  import { app, run } from '../../lib/app.svelte';
  import { groupColor, groupWithFixtures, headCount, moveInGrid, resizeGrid } from '../../lib/show';
  import type { FixtureGroup } from '../../lib/protocol';

  let { selected, groupId, onclose }: { selected: Set<number>; groupId: number; onclose: () => void } = $props();
  const show = $derived(app.s.show!);
  let dragging = $state<number | null>(null);
  let picked = $state<number | null>(null);

  const raw = $derived(show.groups.find((g) => g.id === groupId));
  /** Group with every member inside the grid (older shows may lack positions). */
  const group = $derived.by(() => {
    if (!raw) return undefined;
    const w = raw.grid_width || raw.members.length || 1;
    const h = raw.grid_height || 1;
    const ok = raw.members.every((m) => m.grid && m.grid.x < w && m.grid.y < h);
    return ok && raw.grid_width ? raw : resizeGrid(raw, w, h);
  });
  const color = $derived(group ? groupColor(show, group.id) : 'var(--accent)');

  function save(g: FixtureGroup) {
    run({ type: 'set_group', group: g });
  }

  function drop(x: number, y: number) {
    const from = dragging ?? picked;
    dragging = null;
    picked = null;
    if (group && from !== null) save(moveInGrid(group, from, { x, y }));
  }

  function memberAt(x: number, y: number) {
    return group?.members.findIndex((m) => m.grid?.x === x && m.grid?.y === y) ?? -1;
  }

  function label(i: number) {
    const m = group?.members[i];
    if (!m) return '';
    const f = show.fixtures.find((x) => x.id === m.fixture);
    const heads = f ? headCount(show, f) : 1;
    return `${f?.name ?? m.fixture}${heads > 1 ? ` #${m.head + 1}` : ''}`;
  }

  function addSelected() {
    if (!group) return;
    const next = groupWithFixtures(show, group, [...selected]);
    if (next !== group) save(next);
  }

  function removeMember(i: number) {
    if (group) save({ ...group, members: group.members.filter((_, j) => j !== i) });
  }
</script>

{#if group}
<section class="panel col">
  <div class="row">
    <h2>Group: {group.name}</h2>
    <span class="spacer"></span>
    <button onclick={onclose}>Close</button>
  </div>

    <div class="row">
      <input value={group.name} aria-label="Group name" onchange={(e) => save({ ...group, name: e.currentTarget.value })} />
      <label class="row">Grid
        <input type="number" min="1" max="64" value={group.grid_width} aria-label="Grid width" style="width:64px" onchange={(e) => save(resizeGrid(group, Number(e.currentTarget.value), group.grid_height))} />
        ×
        <input type="number" min="1" max="64" value={group.grid_height} aria-label="Grid height" style="width:64px" onchange={(e) => save(resizeGrid(group, group.grid_width, Number(e.currentTarget.value)))} />
      </label>
      <button disabled={!selected.size} onclick={addSelected}>Add selected</button>
      <button class="danger" onclick={() => { run({ type: 'remove_group', id: group.id }); onclose(); }}>Delete group</button>
    </div>
    <p class="muted small">Drag heads to reorder them: the order (number) follows the grid row by row and drives chases, fans and effect phase.</p>
    <div class="matrix" style="--w:{group.grid_width}; --gc:{color}" role="grid" aria-label="Group matrix">
      {#each Array.from({ length: group.grid_width * group.grid_height }, (_, k) => k) as k (k)}
        {@const x = k % group.grid_width}
        {@const y = Math.floor(k / group.grid_width)}
        {@const i = memberAt(x, y)}
        <div
          class="cell"
          class:target={dragging !== null || picked !== null}
          role="gridcell"
          tabindex="-1"
          ondragover={(e) => e.preventDefault()}
          ondrop={(e) => { e.preventDefault(); drop(x, y); }}
          onclick={() => { if (picked !== null && picked !== i) drop(x, y); }}
          onkeydown={() => {}}
        >
          {#if i >= 0}
            <div
              class="tile"
              class:picked={picked === i}
              draggable="true"
              role="button"
              tabindex="0"
              aria-label="Head {label(i)}"
              ondragstart={(e) => { dragging = i; e.dataTransfer?.setData('text/plain', String(i)); }}
              ondragend={() => (dragging = null)}
              onclick={(e) => { e.stopPropagation(); picked = picked === i ? null : i; }}
              onkeydown={(e) => { if (e.key === 'Enter') picked = picked === i ? null : i; }}
            >
              <span class="order">{i + 1}</span>
              <span class="lbl">{label(i)}</span>
              <button class="rm" aria-label="Remove {label(i)}" onclick={(e) => { e.stopPropagation(); removeMember(i); }}>×</button>
            </div>
          {/if}
        </div>
      {/each}
    </div>
</section>
{/if}

<style>
  .spacer { flex: 1; }
  .small { font-size: 12px; margin: 0; }
  .matrix { display: grid; grid-template-columns: repeat(var(--w), minmax(70px, 1fr)); gap: 6px; overflow-x: auto; }
  .cell { min-height: 64px; border: 1px dashed var(--border); border-radius: var(--radius); display: flex; }
  .cell.target { border-color: var(--accent); }
  .tile { flex: 1; position: relative; background: color-mix(in srgb, var(--gc) 25%, var(--panel-2)); border: 2px solid var(--gc); border-radius: var(--radius); padding: 4px; cursor: grab; display: flex; flex-direction: column; justify-content: center; align-items: center; user-select: none; }
  .tile.picked { outline: 3px solid var(--warn); }
  .order { font-weight: 800; font-size: 16px; }
  .lbl { font-size: 11px; text-align: center; overflow: hidden; text-overflow: ellipsis; max-width: 100%; white-space: nowrap; }
  .rm { position: absolute; top: 2px; right: 2px; padding: 0 5px; font-size: 11px; line-height: 16px; }
</style>
