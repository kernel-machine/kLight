<script lang="ts">
  // Animated preview of a matrix effect on the group's grid.
  import { Matrix, colorToHex } from '../lib/core';
  import type { GroupLayout, MatrixEffect } from '../lib/protocol';

  let { efx, layout, labels, bpm }: { efx: MatrixEffect; layout: GroupLayout; labels: string[]; bpm: number } = $props();
  let turns = $state(0);
  const periodMs = $derived('ms' in efx.period ? efx.period.ms : (efx.period.beats * 60000) / Math.max(1, bpm));

  $effect(() => {
    let raf = 0;
    const loop = (now: number) => {
      // Cycles counted from absolute time (ping-pong needs the count).
      turns = periodMs > 0 ? now / periodMs : 0;
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  // The engine's own algorithms (WebAssembly).
  const matrix = $derived(new Matrix(efx, layout));
  $effect(() => {
    const m = matrix;
    return () => m.free();
  });
  const colors = $derived(matrix.colors(turns).map(colorToHex));
</script>

<div class="matrix" style="--w:{layout.width}; --h:{layout.height}">
  {#each layout.positions as cell, i (i)}
    <div class="cell" style="grid-column:{cell.x + 1}; grid-row:{cell.y + 1}; background:{colors[i]}" title="{i + 1}. {labels[i] ?? ''}"><span>{i + 1}</span></div>
  {:else}
    <span class="muted">This group has no lights: add some in Fixtures → Groups.</span>
  {/each}
</div>
<p class="muted small">{layout.width} × {layout.height} grid, {layout.positions.length} heads, numbered in group order. Their places in the grid are set in Fixtures → Groups.</p>

<style>
  .matrix {
    display: grid;
    grid-template-columns: repeat(var(--w), minmax(0, 1fr));
    grid-template-rows: repeat(var(--h), auto);
    gap: 4px;
    padding: 10px;
    background: #000;
    border-radius: var(--radius);
    max-width: calc(var(--w) * 56px);
  }
  .cell { aspect-ratio: 1; border-radius: 50%; box-shadow: 0 0 10px currentColor; border: 1px solid #222; display: grid; place-items: center; }
  .cell span { font-size: 10px; font-weight: 700; color: #fff; mix-blend-mode: difference; opacity: 0.75; }
  .small { font-size: 12px; }
</style>
