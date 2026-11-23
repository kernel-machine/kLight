<script lang="ts">
  // Patching from the library: manufacturer → model → mode, first free address.
  import { app, run, notify } from '../../lib/app.svelte';
  import { conflictsWith, defKey, nextFreeAddress, nextId } from '../../lib/show';
  import type { FixtureDefinition, LibraryEntry, PatchedFixture } from '../../lib/protocol';

  let { defaultUniverse }: { defaultUniverse: number } = $props();
  const show = $derived(app.s.show!);
  let library = $state<LibraryEntry[]>([]);
  let search = $state('');
  let manufacturer = $state('');
  let modelKey = $state('');
  let modeName = $state('');
  let universe = $state(1);
  let address = $state(1);
  let manualAddress = $state(false);
  let count = $state(1);
  let namePrefix = $state('');
  let force = $state(false);

  $effect(() => {
    universe = defaultUniverse;
  });
  $effect(() => {
    fetch('/api/library')
      .then((r) => (r.ok ? r.json() : []))
      .then((l: LibraryEntry[]) => (library = l))
      .catch(() => (library = []));
  });

  function entryOf(d: FixtureDefinition): LibraryEntry {
    return {
      key: defKey(d),
      manufacturer: d.manufacturer,
      model: d.model,
      kind: d.kind,
      modes: d.modes.map((m) => ({ name: m.name, channels: m.channels.length, heads: Math.max(1, m.heads.length) })),
    };
  }
  const entries = $derived.by(() => {
    const map = new Map<string, LibraryEntry>();
    for (const e of library) map.set(e.key, e);
    for (const d of show.definitions) map.set(defKey(d), entryOf(d));
    return [...map.values()];
  });
  const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: 'base' });
  const matches = (e: LibraryEntry) => {
    const q = search.trim().toLowerCase();
    return !q || e.manufacturer.toLowerCase().includes(q) || e.model.toLowerCase().includes(q);
  };
  const manufacturers = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const e of entries) if (matches(e)) counts.set(e.manufacturer, (counts.get(e.manufacturer) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => byName(a[0], b[0]));
  });
  const models = $derived(entries.filter((e) => e.manufacturer === manufacturer && matches(e)).sort((a, b) => byName(a.model, b.model)));
  const entry = $derived(entries.find((e) => e.key === modelKey));
  const mode = $derived(entry?.modes.find((m) => m.name === modeName) ?? entry?.modes[0]);
  const size = $derived(mode?.channels ?? 0);
  const autoAddress = $derived(size ? nextFreeAddress(show, universe, size * count) : null);
  const preview = $derived(size ? Array.from({ length: count }, (_, i) => conflictsWith(show, universe, address + i * size, size)).flat() : []);
  const fixtureName = (id: number) => show.fixtures.find((f) => f.id === id)?.name ?? `#${id}`;

  $effect(() => {
    if (manufacturers.length === 1 && manufacturer !== manufacturers[0]![0]) manufacturer = manufacturers[0]![0];
  });
  $effect(() => {
    if (entry && !entry.modes.some((m) => m.name === modeName)) modeName = entry.modes[0]?.name ?? '';
  });
  $effect(() => {
    void modelKey;
    void modeName;
    void universe;
    void count;
    manualAddress = false;
  });
  $effect(() => {
    if (!manualAddress && autoAddress !== null) address = autoAddress;
  });

  // Full definition of the selected model, for the channel list.
  let selectedDef = $state<FixtureDefinition | null>(null);
  $effect(() => {
    const key = modelKey;
    selectedDef = null;
    if (!key) return;
    let cancelled = false;
    fullDefinition(key).then((d) => {
      if (!cancelled) selectedDef = d;
    });
    return () => {
      cancelled = true;
    };
  });
  const modeDef = $derived(selectedDef?.modes.find((m) => m.name === (mode?.name ?? '')) ?? null);
  /** Mode channel index → head number (1-based), for multi-head fixtures. */
  const headOf = $derived.by(() => {
    const map = new Map<number, number>();
    modeDef?.heads.forEach((h, i) => h.channels.forEach((c) => map.set(c, i + 1)));
    return map;
  });

  async function fullDefinition(key: string): Promise<FixtureDefinition | null> {
    const embedded = show.definitions.find((d) => defKey(d) === key);
    if (embedded) return embedded;
    try {
      const r = await fetch(`/api/library/definition?key=${encodeURIComponent(key)}`);
      return r.ok ? ((await r.json()) as FixtureDefinition) : null;
    } catch {
      return null;
    }
  }

  async function patch() {
    if (!entry || !mode) return;
    // Freeze the form: every patched fixture changes the show, which moves
    // the "first free address" while the loop is still running.
    const key = entry.key;
    const modeName = mode.name;
    const base = address;
    const step = size;
    const n = count;
    const target = universe;
    const prefix = namePrefix;
    const forced = force;
    const def = await fullDefinition(key);
    if (!def) {
      notify(`Definition ${key} not available`, 'error');
      return;
    }
    if (!show.definitions.some((d) => defKey(d) === key)) {
      if (!(await run({ type: 'import_definition', definition: def }))) return;
    }
    const firstId = nextId(show.fixtures);
    for (let i = 0; i < n; i++) {
      const fixture: PatchedFixture = {
        id: firstId + i,
        name: `${prefix || def.model} ${n > 1 ? i + 1 : ''}`.trim(),
        definition: key,
        mode: modeName,
        universe: target,
        address: base + i * step,
        orientation: { invert_pan: false, invert_tilt: false, swap_pan_tilt: false, upside_down: false },
        calibration: null,
      };
      if (!(await run({ type: 'patch_fixture', fixture, force: forced }))) return;
    }
    notify(`Patched ${n} × ${def.model}`, 'info');
    manualAddress = false;
    force = false;
  }
</script>

<section class="panel col">
  <h2>Add fixtures</h2>
  <input placeholder="Search manufacturer or model…" bind:value={search} />
  <div class="pickers">
    <label class="col">
      <span class="muted">Manufacturer ({manufacturers.length})</span>
      <select size="8" bind:value={manufacturer} aria-label="Manufacturer">
        {#each manufacturers as [name, n] (name)}<option value={name}>{name} ({n})</option>{/each}
      </select>
    </label>
    <label class="col">
      <span class="muted">Model</span>
      <select size="8" bind:value={modelKey} aria-label="Model">
        {#each models as e (e.key)}<option value={e.key}>{e.model}</option>{/each}
      </select>
    </label>
  </div>
  {#if entry}
    <div class="row">
      <label>Mode <select bind:value={modeName}>{#each entry.modes as m (m.name)}<option value={m.name}>{m.name} ({m.channels} ch)</option>{/each}</select></label>
      <span class="muted">{entry.kind}{mode && mode.heads > 1 ? ` · ${mode.heads} heads` : ''}</span>
    </div>
    <div class="channels" aria-label="Mode channels">
      {#if modeDef && selectedDef}
        <table>
          <thead><tr><th>Ch</th><th>Name</th><th>Function</th>{#if headOf.size}<th>Head</th>{/if}</tr></thead>
          <tbody>
            {#each modeDef.channels as ci, i (i)}
              {@const ch = selectedDef.channels[ci]}
              <tr>
                <td class="num">{i + 1}</td>
                <td>{ch?.name ?? `Channel ${i + 1}`}{ch?.byte === 'Fine' ? ' (fine)' : ''}</td>
                <td class="muted">{ch?.attribute ?? ''}</td>
                {#if headOf.size}<td class="muted">{headOf.get(i) ?? '—'}</td>{/if}
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <span class="muted">Loading channels…</span>
      {/if}
    </div>
    <div class="row">
      <label>Universe <select bind:value={universe}>{#each show.universes as u (u.id)}<option value={u.id}>{u.id}</option>{/each}</select></label>
      <label>Address <input type="number" min="1" max="512" bind:value={address} oninput={() => (manualAddress = true)} style="width:80px" /></label>
      <button disabled={!manualAddress} onclick={() => (manualAddress = false)}>First free</button>
      <label>Count <input type="number" min="1" max="64" bind:value={count} style="width:60px" /></label>
    </div>
    {#if autoAddress === null && size}
      <div class="badge danger">No free block of {size * count} channels in universe {universe}</div>
    {/if}
    <input placeholder="Name (default: model)" bind:value={namePrefix} />
    {#if preview.length}
      <div class="badge danger">Conflicts with {[...new Set(preview)].map(fixtureName).join(', ')}</div>
      <label class="row"><input type="checkbox" bind:checked={force} /> Patch anyway</label>
    {/if}
    <button class="primary" disabled={!mode || (preview.length > 0 && !force)} onclick={patch}>Patch</button>
  {:else}
    <span class="muted">{entries.length ? 'Pick a manufacturer and a model' : 'No fixture library found (put .qxf files in fixtures/)'}</span>
  {/if}
</section>

<style>
  .channels { max-height: 260px; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius); }
  .channels table { font-size: 13px; }
  .channels th { position: sticky; top: 0; background: var(--panel); }
  .channels td, .channels th { padding: 3px 8px; }
  .num { font-variant-numeric: tabular-nums; color: var(--muted); width: 40px; }
  .pickers { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .pickers select { width: 100%; }
</style>
