<script lang="ts">
  // Times of a scene: fade in, fade out and delay; the delay spread over the
  // order of its lights (one after the other, or from the center out); and
  // different times for some attributes (e.g. the color snaps while the
  // intensity fades). The engine applies them (`timing_for`, spread in
  // playback): the editor only stores them.
  import { formatTime, parseTime } from '../lib/show';
  import type { AttributeKind, AttributeTiming, Scene, Spread, TimeValue, Timing } from '../lib/protocol';

  let { scene, onchange }: { scene: Scene; onchange: (change: (s: Scene) => Scene) => void } = $props();

  const ZERO: TimeValue = { ms: 0 };
  const KINDS: Array<[AttributeKind, string]> = [
    ['Intensity', 'Intensity'],
    ['Color', 'Color'],
    ['Position', 'Position'],
    ['Beam', 'Beam'],
  ];
  const FIELDS: Array<[keyof Timing, string]> = [
    ['fade_in', 'Fade in'],
    ['fade_out', 'Fade out'],
    ['delay', 'Delay'],
  ];

  function ask(current: TimeValue, apply: (t: TimeValue) => void) {
    const text = window.prompt('Time (e.g. 500, 1.5s, 2 beats)', formatTime(current));
    if (text === null) return;
    const t = parseTime(text);
    if (t) apply(t);
  }

  // Open where the scene has some, then as the operator leaves it.
  let showOwn = $state(false);
  let opened = false;
  $effect.pre(() => {
    if (!opened && scene.attribute_timing.length) showOwn = true;
    opened = true;
  });

  // Folded into one line of summary: set rarely, and last.
  let showAll = $state(false);
  const spreadText = $derived(
    scene.spread.type === 'none' ? 'no delay over the lights' : `delay ${scene.spread.type === 'linear' ? 'one after the other' : 'from the center'}, ${formatTime(scene.spread.step)}`,
  );
  const summary = $derived(
    [
      `Fade in ${formatTime(scene.timing.fade_in)}`,
      `fade out ${formatTime(scene.timing.fade_out)}`,
      `delay ${formatTime(scene.timing.delay)}`,
      spreadText,
      ...(scene.attribute_timing.length ? [`own times: ${scene.attribute_timing.map((a) => a.kind.toLowerCase()).join(', ')}`] : []),
    ].join(' · '),
  );

  const isZero = (t: TimeValue) => ('ms' in t ? t.ms === 0 : t.beats === 0);
  const spreadStep = $derived(scene.spread.type === 'none' ? ZERO : scene.spread.step);

  function setSpread(type: Spread['type'], step: TimeValue) {
    const spread: Spread = type === 'none' || isZero(step) ? { type: 'none' } : { type, step };
    onchange((s) => ({ ...s, spread }));
  }
  function pickSpreadType(type: Spread['type']) {
    // A spread without a step does nothing: one beat to start from.
    setSpread(type, type !== 'none' && isZero(spreadStep) ? { beats: 0.25 } : spreadStep);
  }

  const own = (kind: AttributeKind) => scene.attribute_timing.find((a) => a.kind === kind);
  function setOwn(kind: AttributeKind, timing: Timing | null) {
    onchange((s) => {
      const rest = s.attribute_timing.filter((a) => a.kind !== kind);
      const next: AttributeTiming[] = timing ? [...rest, { kind, timing }] : rest;
      // In the order of the list, so the stored show does not depend on clicks.
      next.sort((a, b) => KINDS.findIndex(([k]) => k === a.kind) - KINDS.findIndex(([k]) => k === b.kind));
      return { ...s, attribute_timing: next };
    });
  }
</script>

<details class="timing" bind:open={showAll}>
<summary><span class="arrow" aria-hidden="true">{showAll ? '▾' : '▸'}</span><span class="title">Timing</span> <span class="muted summary">{summary}</span></summary>
<div class="row">
  {#each FIELDS as [key, label] (key)}
    <button onclick={() => ask(scene.timing[key], (t) => onchange((s) => ({ ...s, timing: { ...s.timing, [key]: t } })))}>{label}: {formatTime(scene.timing[key])}</button>
  {/each}
</div>

<div class="row">
  <label class="row">
    Delay over the lights
    <select aria-label="Delay over the lights" value={scene.spread.type} onchange={(e) => pickSpreadType(e.currentTarget.value as Spread['type'])}>
      <option value="none">off</option>
      <option value="linear">one after the other</option>
      <option value="from_center">from the center out</option>
    </select>
  </label>
  {#if scene.spread.type !== 'none'}
    {@const type = scene.spread.type}
    <button onclick={() => ask(spreadStep, (t) => setSpread(type, t))}>Step: {formatTime(spreadStep)}</button>
    <span class="muted small">in the order of each group, or of the lights for values to all lights</span>
  {/if}
</div>

<details class="own" bind:open={showOwn}>
  <summary>Times per attribute {#if scene.attribute_timing.length}<span class="muted">({scene.attribute_timing.length})</span>{/if}</summary>
  <p class="muted small">An attribute with its own times ignores the times above, e.g. the color changes at once while the intensity fades.</p>
  <div class="kinds">
      {#each KINDS as [kind, label] (kind)}
        {@const t = own(kind)?.timing}
        <div class="kind">
            <label class="row" title={kind === 'Beam' ? 'Gobo, prism, zoom and strobe' : undefined}>
              <input type="checkbox" checked={!!t} aria-label="{label}: own times" onchange={(e) => setOwn(kind, e.currentTarget.checked ? { ...scene.timing } : null)} />
              {label}
            </label>
          <div class="times">
            {#if t}
              {#each FIELDS as [key, name] (key)}
                <button onclick={() => ask(t[key], (v) => setOwn(kind, { ...t, [key]: v }))}>{name}: {formatTime(t[key])}</button>
              {/each}
            {:else}
              <span class="muted small">as the scene</span>
            {/if}
          </div>
        </div>
      {/each}
  </div>
</details>
</details>

<style>
  .timing { margin-top: 14px; border: 1px solid var(--border); border-radius: var(--radius); padding: 0 12px; background: var(--panel-2); }
  .timing > summary { cursor: pointer; padding: 10px 0; display: flex; gap: 10px; align-items: baseline; list-style: none; }
  .timing > summary::-webkit-details-marker { display: none; }
  .arrow { color: var(--muted); width: 10px; }
  .timing[open] > summary { border-bottom: 1px solid var(--border); margin-bottom: 8px; }
  .timing .title { font-weight: 700; font-size: 15px; }
  .timing .summary { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .timing > .row { margin-bottom: 8px; }
  .own { margin: 8px 0 12px; }
  .own summary { cursor: pointer; font-weight: 600; }
  .kinds { display: flex; flex-direction: column; gap: 4px; }
  .kind label { white-space: nowrap; }
  .kind { display: grid; grid-template-columns: minmax(140px, 1fr) 3fr; align-items: center; gap: 8px; }
  .times { display: flex; flex-wrap: wrap; gap: 4px; }
  .times button { font-size: 12px; padding: 4px 8px; }
  .small { font-size: 12px; }
</style>
