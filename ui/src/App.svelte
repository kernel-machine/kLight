<script lang="ts">
  import { app, pref, setPref } from './lib/app.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import MasterBar from './components/MasterBar.svelte';
  import DmxMonitor from './components/DmxMonitor.svelte';
  import Notices from './components/Notices.svelte';
  import Fixtures from './screens/Fixtures.svelte';
  import Fx from './screens/Fx.svelte';
  import VirtualConsole from './screens/VirtualConsole.svelte';
  import SimpleDesk from './screens/SimpleDesk.svelte';
  import Devices from './screens/Devices.svelte';

  const tabs = [
    { id: 'fixtures', label: 'Fixtures' },
    { id: 'fx', label: 'FX' },
    { id: 'vc', label: 'Virtual Console' },
    { id: 'desk', label: 'Simple Desk' },
    { id: 'devices', label: 'Devices' },
  ] as const;
  type TabId = (typeof tabs)[number]['id'];

  let tab = $state<TabId>(pref<TabId>('tab', 'vc'));
  let monitor = $state(false);
  let theme = $state(pref<'dark' | 'light'>('theme', 'dark'));

  $effect(() => setPref('tab', tab));
  $effect(() => {
    document.documentElement.dataset.theme = theme;
    setPref('theme', theme);
  });
</script>

<div class="shell">
  <header>
    <div class="brand">kLight</div>
    <nav>
      {#each tabs as t (t.id)}
        <button class:active={tab === t.id} onclick={() => (tab = t.id)}>{t.label}</button>
      {/each}
    </nav>
    <div class="row">
      <button onclick={() => (monitor = !monitor)} class:active={monitor}>DMX monitor</button>
      <button onclick={() => (theme = theme === 'dark' ? 'light' : 'dark')} title="Theme">◐</button>
    </div>
  </header>
  <div class="body">
  <main>
    {#if !app.s.show}
      <div class="panel waiting">
        {#if app.s.connection === 'connected'}Loading show…{:else}Connecting to the engine…{/if}
      </div>
    {:else if tab === 'fixtures'}
      <Fixtures />
    {:else if tab === 'fx'}
      <Fx />
    {:else if tab === 'vc'}
      <VirtualConsole />
    {:else if tab === 'desk'}
      <SimpleDesk />
    {:else}
      <Devices />
    {/if}
    {#if monitor && app.s.show}
      <DmxMonitor onclose={() => (monitor = false)} />
    {/if}
  </main>
  <MasterBar />
  </div>
  <StatusBar />
  <Notices />
</div>

<style>
  .shell { display: grid; grid-template-rows: auto 1fr auto; height: 100%; }
  .body { display: flex; min-height: 0; }
  header { display: flex; align-items: center; gap: 16px; padding: 8px 12px; background: var(--panel); border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .brand { font-weight: 800; letter-spacing: 0.5px; font-size: 18px; }
  nav { display: flex; gap: 6px; flex: 1; flex-wrap: wrap; }
  main { flex: 1; min-width: 0; overflow: auto; padding: 12px; position: relative; }
  .waiting { text-align: center; padding: 40px; color: var(--muted); }
</style>
