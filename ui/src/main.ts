import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { client } from './lib/app.svelte';
import { initCore } from './lib/core';

// The core's WebAssembly functions must be ready before the first render.
initCore()
  .then(() => {
    client.connect();
    const target = document.getElementById('app');
    if (!target) throw new Error('missing #app');
    mount(App, { target });
  })
  .catch((e: unknown) => {
    document.body.textContent = `kLight could not start: ${String(e)}`;
  });
