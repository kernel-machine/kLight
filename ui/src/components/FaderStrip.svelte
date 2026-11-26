<script lang="ts">
  // One vertical channel fader with its labels, as in the Simple Desk and in
  // the scene editor.
  import ChannelIcon from './ChannelIcon.svelte';
  import type { Attribute } from '../lib/protocol';

  let {
    address,
    fixture,
    channel,
    attribute,
    value,
    label,
    subject = label,
    title,
    patched = true,
    alt = false,
    first = false,
    set = false,
    scene = false,
    unset = false,
    paired = false,
    disabled = false,
    now = undefined,
    onchoose,
    oninput,
    onslideend,
    onentry,
    onrelease,
  }: {
    /** Top line: the DMX address. */
    address: string | number;
    /** Fixture name (shown on its first channel only). */
    fixture: string;
    channel: string;
    attribute: Attribute | null;
    value: number;
    /** Accessible name of the controls. */
    label: string;
    /** How the channel is named inside "Presets of …" and "Release …". */
    subject?: string;
    title: string;
    patched?: boolean;
    /** Alternate tint, to tell neighbouring fixtures apart. */
    alt?: boolean;
    /** First channel of its fixture. */
    first?: boolean;
    /** Set by hand: it can be released. */
    set?: boolean;
    /** Set by a scene value. */
    scene?: boolean;
    /** Nothing sets it (default value). */
    unset?: boolean;
    /** Driven only because the other byte of its channel is set by hand. */
    paired?: boolean;
    disabled?: boolean;
    /** Value on the output right now, when shown. */
    now?: number | null;
    /** The channel has named ranges: opens their list. */
    onchoose?: (e: MouseEvent) => void;
    /** While the fader moves. */
    oninput: (value: number) => void;
    /** When the fader is let go. */
    onslideend?: (value: number) => void;
    /** A value typed in the number field. */
    onentry: (value: number) => void;
    onrelease: () => void;
  } = $props();

  const clamp = (text: string) => Math.max(0, Math.min(255, Math.round(Number(text) || 0)));
</script>

<div class="strip" class:set class:scene class:unset class:paired class:patched class:alt class:first {title}>
  <div class="addr">{address}</div>
  {#if attribute !== null && onchoose}
    <button class="icon choose" title="Choose: {channel}" aria-label="Presets of {subject}" aria-haspopup="listbox" {disabled} onclick={onchoose}>
      <ChannelIcon {attribute} /><span class="caret">▾</span>
    </button>
  {:else}
    <div class="icon">{#if attribute !== null}<ChannelIcon {attribute} />{/if}</div>
  {/if}
  <div class="name">{fixture}</div>
  <div class="ch">{channel}</div>
  <input
    class="fader"
    type="range"
    min="0"
    max="255"
    step="1"
    {value}
    {disabled}
    aria-label={label}
    oninput={(e) => oninput(Number(e.currentTarget.value))}
    onchange={(e) => onslideend?.(Number(e.currentTarget.value))}
  />
  <input
    class="num"
    type="number"
    min="0"
    max="255"
    {value}
    {disabled}
    aria-label="{label} value"
    onchange={(e) => onentry(clamp(e.currentTarget.value))}
  />
  {#if now !== undefined}
    <div class="now" class:differs={now !== null && now !== value} title="Value on the output now">{now ?? '—'}</div>
  {/if}
  <button class="release" disabled={!set} title="Release" aria-label="Release {subject}" onclick={onrelease}>×</button>
</div>

<style>
  .strip { display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 4px 2px; border-radius: var(--radius); background: var(--panel); border: 1px solid var(--border); min-width: 0; }
  .strip.patched { background: color-mix(in srgb, var(--accent) 8%, var(--panel)); }
  .strip.patched.alt { background: color-mix(in srgb, var(--ok) 8%, var(--panel)); }
  .strip.first { border-left: 3px solid var(--accent); }
  .strip.scene { border-color: var(--accent); }
  .strip.paired { border-style: dashed; border-color: var(--warn); }
  .strip.set { border-color: var(--warn); box-shadow: inset 0 0 0 1px var(--warn); }
  .strip.unset input { opacity: 0.55; }
  .icon { height: 16px; }
  .choose { display: inline-flex; align-items: center; gap: 1px; height: 18px; padding: 0 3px; border: 1px solid var(--border); border-radius: 4px; background: transparent; cursor: pointer; }
  .choose:hover { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 15%, transparent); }
  .caret { font-size: 8px; color: var(--muted); }
  .addr { font-weight: 700; font-variant-numeric: tabular-nums; font-size: 12px; }
  .name, .ch { font-size: 9px; color: var(--muted); width: 100%; text-align: center; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; height: 11px; }
  .name { color: var(--text); }
  .fader { height: 130px; width: 36px; --cap-h: 20px; }
  .num { width: 48px; padding: 2px; text-align: center; font-size: 12px; }
  .now { font-size: 10px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .now.differs { color: var(--warn); font-weight: 700; }
  .release { padding: 0 8px; font-size: 12px; }
</style>
