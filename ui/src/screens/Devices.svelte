<script lang="ts">
  import { app, client, run, notify } from '../lib/app.svelte';
  import { describeInputMessage as describeMsg, nextId } from '../lib/show';
  import type { InputDeviceConfig, InputMapping, InputProfileInfo, InputTarget, LedBehavior, OutputKind, SerialPortInfo, Universe } from '../lib/protocol';

  const show = $derived(app.s.show!);
  const status = $derived(app.s.status);
  let ports = $state<SerialPortInfo[]>([]);
  let profiles = $state<InputProfileInfo[]>([]);
  async function loadProfiles() {
    try {
      const r = await fetch('/api/input-profiles');
      profiles = r.ok ? await r.json() : [];
    } catch {
      profiles = [];
    }
  }
  $effect(() => {
    loadProfiles();
  });
  const profileOf = (d: InputDeviceConfig) => profiles.find((p) => p.id === d.profile);

  /** Menu value of an LED behaviour: '' = profile default, 'off', or 'c<channel>'. */
  const lookKey = (b: LedBehavior | null | undefined) => (!b ? '' : b.type === 'off' ? 'off' : `c${b.channel}`);
  const lookFromKey = (key: string): LedBehavior | null =>
    key === '' ? null : key === 'off' ? { type: 'off' } : { type: 'channel', channel: Number(key.slice(1)) };
  function setLook(i: number, field: 'feedback_active' | 'feedback_inactive', key: string) {
    saveDevices(show.input_devices.map((x, j) => (j === i ? { ...x, [field]: lookFromKey(key) } : x)));
  }

  let learning = $state(false);
  let learnTarget = $state<string>('grand_master');

  async function refreshPorts() {
    try {
      const r = await fetch('/api/ports');
      ports = r.ok ? await r.json() : [];
    } catch {
      ports = [];
    }
  }
  $effect(() => {
    refreshPorts();
  });

  function assign(u: Universe, kind: OutputKind | '', serial: string) {
    const output = kind === '' ? null : { kind, serial_number: serial || null, port_hint: null };
    run({ type: 'set_universe_output', id: u.id, output });
  }

  function outStatus(id: number) {
    return status?.outputs.find((o) => o.universe === id);
  }

  function saveDevices(devices: InputDeviceConfig[]) {
    run({ type: 'set_input_devices', devices });
  }

  function targetFromKey(key: string): InputTarget {
    if (key.startsWith('w:')) return { type: 'widget', id: Number(key.slice(2)) };
    return { type: key } as InputTarget;
  }

  function describeTarget(t: InputTarget) {
    if (t.type === 'widget') return `Widget "${allWidgets().find((w) => w.id === t.id)?.props.label ?? t.id}"`;
    return t.type.replace('_', ' ');
  }

  function allWidgets() {
    const out: { id: number; props: { label: string } }[] = [];
    show.virtual_console.widgets.forEach(function walk(w) {
      out.push(w);
      w.children.forEach(walk);
    });
    return out;
  }

  async function learn() {
    learning = true;
    notify('Move a control on the controller…', 'info');
    const l = await client.learn();
    learning = false;
    const mapping: InputMapping = { id: nextId(show.input_mappings), device: l.device, message: l.message, target: targetFromKey(learnTarget) };
    await run({ type: 'set_input_mappings', mappings: [...show.input_mappings, mapping] });
    notify(`Mapped ${describeMsg(l.message)} on ${l.device}`, 'info');
  }

  let newOscName = $state('OSC');
  let newOscPort = $state(9000);
</script>

<div class="grid2">
  <section class="panel">
    <div class="row"><h2>Outputs</h2><button onclick={refreshPorts}>Rescan USB</button></div>
    <table>
      <thead><tr><th>Universe</th><th>Interface</th><th>Serial number</th><th>Status</th></tr></thead>
      <tbody>
        {#each show.universes as u (u.id)}
          {@const st = outStatus(u.id)}
          <tr>
            <td>{u.id}: {u.name}</td>
            <td>
              <select value={u.output?.kind ?? ''} onchange={(e) => assign(u, e.currentTarget.value as OutputKind | '', u.output?.serial_number ?? ports[0]?.serial_number ?? '')}>
                <option value="">— none —</option>
                <option value="EnttecPro">Enttec DMX USB Pro</option>
                <option value="OpenDmx">Enttec Open DMX</option>
                <option value="Fake">Simulated</option>
              </select>
            </td>
            <td>
              <select value={u.output?.serial_number ?? ''} disabled={!u.output} onchange={(e) => u.output && assign(u, u.output.kind, e.currentTarget.value)}>
                <option value="">—</option>
                {#each ports.filter((p) => p.serial_number) as p (p.port)}
                  <option value={p.serial_number}>{p.serial_number} ({p.product ?? p.port}){p.is_ftdi ? '' : ' · not FTDI'}</option>
                {/each}
                {#if u.output?.serial_number && !ports.some((p) => p.serial_number === u.output?.serial_number)}
                  <option value={u.output.serial_number}>{u.output.serial_number} (not connected)</option>
                {/if}
              </select>
            </td>
            <td>
              {#if st}
                <span class="badge {st.state === 'ok' ? 'ok' : 'danger'}">{st.state}</span>
                {#if !st.hardware_buffer}<span class="badge warn" title="If the engine stops, the output stops">no hardware buffer</span>{/if}
                <span class="muted">{st.frames_sent} frames · {st.errors} errors</span>
                {#if st.last_error}<div class="muted">{st.last_error}</div>{/if}
              {:else}<span class="muted">—</span>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="muted">Interfaces are matched by USB serial number: plugging them into another port keeps the universe.</p>
  </section>

  <section class="panel">
    <h2>Inputs</h2>
    <h3>Detected controllers</h3>
    {#each status?.inputs ?? [] as i (i.name)}
      <div class="row"><span class="badge {i.connected ? 'ok' : 'danger'}">{i.kind}</span> {i.name}</div>
    {:else}<span class="muted">No MIDI/OSC devices.</span>{/each}
    <h3>Configured devices (feedback profile, OSC)</h3>
    {#each show.input_devices as d, i (i)}
      <div class="row">
        <b>{d.name}</b> <span class="muted">{d.kind.type === 'osc' ? `OSC :${d.kind.port}` : 'MIDI'}</span>
        <select aria-label="Controller profile" value={d.profile ?? ''} onchange={(e) => saveDevices(show.input_devices.map((x, j) => (j === i ? { ...x, profile: e.currentTarget.value || null, feedback_active: null, feedback_inactive: null } : x)))}>
          <option value="">— no LED feedback —</option>
          {#each profiles as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
          {#if d.profile && !profileOf(d)}<option value={d.profile}>{d.profile} (not found)</option>{/if}
        </select>
        <button onclick={() => saveDevices(show.input_devices.filter((_, j) => j !== i))}>×</button>
      </div>
      {@const prof = profileOf(d)}
      {#if prof && prof.behaviors.length}
        <div class="row look">
          <label>When on (function running)
            <select value={lookKey(d.feedback_active)} onchange={(e) => setLook(i, 'feedback_active', e.currentTarget.value)}>
              <option value="">Default{prof.behaviors.find((b) => b.channel === prof.default_active) ? ` (${prof.behaviors.find((b) => b.channel === prof.default_active)?.label})` : ''}</option>
              {#each prof.behaviors as b (b.channel)}<option value={`c${b.channel}`}>{b.label}</option>{/each}
            </select>
          </label>
          <label>When off
            <select value={lookKey(d.feedback_inactive)} onchange={(e) => setLook(i, 'feedback_inactive', e.currentTarget.value)}>
              <option value="">Off</option>
              {#each prof.behaviors as b (b.channel)}<option value={`c${b.channel}`}>{b.label}</option>{/each}
            </select>
          </label>
        </div>
      {/if}
    {/each}
    <div class="row">
      {#each (status?.inputs ?? []).filter((i) => i.kind === 'midi' && !show.input_devices.some((d) => d.name === i.name)) as i (i.name)}
        <button onclick={() => saveDevices([...show.input_devices, { name: i.name, kind: { type: 'midi' }, profile: null, feedback_active: null, feedback_inactive: null }])}>+ {i.name}</button>
      {/each}
    </div>
    <div class="row">
      <input bind:value={newOscName} style="width:120px" /> <input type="number" bind:value={newOscPort} style="width:90px" />
      <button onclick={() => saveDevices([...show.input_devices, { name: newOscName, kind: { type: 'osc', port: newOscPort }, profile: null, feedback_active: null, feedback_inactive: null }])}>+ OSC listener</button>
    </div>
    <h3>Mappings</h3>
    <table>
      <tbody>
        {#each show.input_mappings as m (m.id)}
          <tr><td>{m.device}</td><td>{describeMsg(m.message)}</td><td>→ {describeTarget(m.target)}</td>
            <td><button onclick={() => run({ type: 'set_input_mappings', mappings: show.input_mappings.filter((x) => x.id !== m.id) })}>×</button></td></tr>
        {/each}
      </tbody>
    </table>
    <div class="row" style="margin-top:8px">
      <select bind:value={learnTarget}>
        <option value="grand_master">Grand master</option>
        <option value="blackout">Blackout</option>
        <option value="tap_tempo">Tap tempo</option>
        {#each allWidgets() as w (w.id)}<option value={`w:${w.id}`}>Widget "{w.props.label}"</option>{/each}
      </select>
      <button class="primary" disabled={learning} onclick={learn}>{learning ? 'Waiting for input…' : 'MIDI learn'}</button>
    </div>
  </section>
</div>
