import { describe, expect, it } from 'vitest';
import { EngineClient, defaultUrl, type SocketLike } from './client';
import type { ServerMessage } from './protocol';

class FakeSocket implements SocketLike {
  sent: unknown[] = [];
  onopen: ((ev: unknown) => void) | null = null;
  onclose: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  send(data: string) {
    this.sent.push(JSON.parse(data));
  }
  close() {
    this.onclose?.({});
  }
  push(m: ServerMessage) {
    this.onmessage?.({ data: JSON.stringify(m) });
  }
}

function setup() {
  const sockets: FakeSocket[] = [];
  const scheduled: Array<() => void> = [];
  const c = new EngineClient(
    'ws://x/ws',
    () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    'tok',
    (f) => scheduled.push(f),
  );
  c.connect();
  const s = sockets[0]!;
  s.onopen?.({});
  s.push({ type: 'welcome', client: 1, permission: 'full', protocol: 1, server: 'test' });
  return { c, s, sockets, scheduled };
}

describe('EngineClient', () => {
  it('sends hello with token and becomes connected', () => {
    const { c, s } = setup();
    expect(s.sent[0]).toEqual({ type: 'hello', name: 'klight-ui', token: 'tok' });
    expect(c.state.connection).toBe('connected');
  });

  it('resolves commands on ack and rejects on error', async () => {
    const { c, s } = setup();
    const ok = c.command({ type: 'set_blackout', on: true });
    const bad = c.command({ type: 'start_function', id: 9 });
    s.push({ type: 'ack', seq: 1, id: 1, warnings: ['w'], show_seq: 0 });
    s.push({ type: 'error', seq: 2, id: 2, error: { kind: 'command', error: { type: 'not_found', what: 'function 9' } } });
    await expect(ok).resolves.toEqual(['w']);
    await expect(bad).rejects.toThrow('function 9 not found');
  });

  it('requests a snapshot when a sequence gap is detected', () => {
    const { c, s } = setup();
    s.push({ type: 'pong', seq: 1 });
    s.push({ type: 'pong', seq: 5 });
    expect(c.state.resyncs).toBe(1);
    expect(s.sent.at(-1)).toEqual({ type: 'request_snapshot' });
  });

  it('reconnects with backoff after the socket closes', () => {
    const { c, s, sockets, scheduled } = setup();
    s.close();
    expect(c.state.connection).toBe('disconnected');
    expect(scheduled.length).toBe(1);
    scheduled[0]!();
    expect(sockets.length).toBe(2);
  });

  it('builds the websocket URL', () => {
    expect(defaultUrl({ protocol: 'http:', host: 'tablet:9400' })).toBe('ws://tablet:9400/ws');
    expect(defaultUrl({ protocol: 'tauri:', host: 'localhost' })).toBe('ws://127.0.0.1:9400/ws');
  });

  it('resolves an edit only after the show that contains it arrived', async () => {
    const { c, s } = setup();
    let done = false;
    const p = c.command({ type: 'set_show_name', name: 'x' }).then(() => (done = true));
    s.push({ type: 'ack', seq: 1, id: 1, warnings: [], show_seq: 5 });
    await Promise.resolve();
    expect(done).toBe(false);
    s.push({ type: 'show_changed', seq: 2, show_seq: 5, show: { name: 'x' } as never });
    await p;
    expect(done).toBe(true);
  });

  it('merges DMX watchers', () => {
    const { c, s } = setup();
    const a = c.watchDmx([1]);
    const b = c.watchDmx([1, 2]);
    expect(s.sent.at(-1)).toEqual({ type: 'subscribe_dmx', universes: [1, 2] });
    b();
    expect(s.sent.at(-1)).toEqual({ type: 'subscribe_dmx', universes: [1] });
    a();
    expect(s.sent.at(-1)).toEqual({ type: 'subscribe_dmx', universes: [] });
  });
});
