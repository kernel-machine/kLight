/// <reference types="node" />
// Vitest runs in Node: load the core WebAssembly module from disk.
import { readFileSync } from 'node:fs';
import { initCoreSync } from './lib/core';

initCoreSync(readFileSync(new URL('./lib/wasm/klight_wasm_bg.wasm', import.meta.url)));
