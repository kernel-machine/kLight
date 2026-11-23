import { defineConfig } from '@playwright/test';

// Main-flow tests against a real engine serving the built UI:
//   cargo build -p klight-engine && npm run build && npm run e2e
// Uses the system Chrome when PW_CHANNEL=chrome (no browser download needed).
export default defineConfig({
  testDir: 'e2e',
  timeout: 30_000,
  use: {
    baseURL: 'http://127.0.0.1:9411',
    channel: process.env.PW_CHANNEL || undefined,
  },
  webServer: {
    command:
      'cd .. && rm -rf target/e2e && mkdir -p target/e2e && ./target/debug/klight-engine --show target/e2e/show.json --bind 127.0.0.1:9411 --ui ui/dist --fixtures fixtures',
    url: 'http://127.0.0.1:9411/api/health',
    reuseExistingServer: false,
  },
});
