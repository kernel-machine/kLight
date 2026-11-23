// WebSocket client of the engine. The core is the only source of truth:
// this class only mirrors what the engine publishes and sends commands.

import type {
  ClientMessage,
  Command,
  Permission,
  RuntimeSnapshot,
  ServerMessage,
  Show,
  SystemStatus,
  ApiError,
  InputMessage,
} from './protocol';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected';

export interface ClientState {
  connection: ConnectionState;
  permission: Permission | null;
  show: Show | null;
  runtime: RuntimeSnapshot | null;
  status: SystemStatus | null;
  dmx: Map<number, number[]>;
  /** Number of sequence gaps detected (each triggers a resync). */
  resyncs: number;
}

export interface SocketLike {
  send(data: string): void;
  close(): void;
  onopen: ((ev: unknown) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
}

export type SocketFactory = (url: string) => SocketLike;

export class CommandFailed extends Error {
  constructor(public readonly error: ApiError) {
    super(describeError(error));
  }
}

export function describeError(e: ApiError): string {
  switch (e.kind) {
    case 'command': {
      const c = e.error;
      switch (c.type) {
        case 'not_found':
          return `${c.what} not found`;
        case 'invalid_value':
          return `Invalid value: ${c.reason}`;
        case 'address_conflicts':
          return `Address conflict: ${c.conflicts
            .map((x) => `fixtures ${x.first} and ${x.second} at ${x.address}`)
            .join(', ')}`;
        case 'patch':
          return `Patch error: ${JSON.stringify(c.error)}`;
        case 'show_mode_locked':
          return 'Show mode: structural edits are locked';
        case 'bad_pin':
          return 'Wrong PIN';
        default:
          return c.type.replace(/_/g, ' ');
      }
    }
    case 'malformed':
      return `Malformed message: ${e.reason}`;
    default:
      return e.kind;
  }
}

interface Pending {
  resolve: (warnings: string[]) => void;
  reject: (e: CommandFailed) => void;
}

export class EngineClient {
  state: ClientState = {
    connection: 'disconnected',
    permission: null,
    show: null,
    runtime: null,
    status: null,
    dmx: new Map(),
    resyncs: 0,
  };
  private socket: SocketLike | null = null;
  private lastSeq = 0;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private listeners = new Set<(s: ClientState) => void>();
  private learnWaiters: Array<(l: { device: string; message: InputMessage }) => void> = [];
  /** Version of the show we hold (from snapshots and show updates). */
  private showSeq = 0;
  /** Acknowledged commands waiting for the show version that includes them. */
  private awaitingShow: Array<{ seq: number; resolve: () => void }> = [];
  private retryMs = 250;
  private closed = false;
  private dmxSubscription: number[] = [];
  private dmxWatchers = new Map<number, number>();

  constructor(
    private readonly url: string,
    private readonly factory: SocketFactory,
    private readonly token: string | null = null,
    private readonly scheduler: (f: () => void, ms: number) => void = (f, ms) => {
      setTimeout(f, ms);
    },
  ) {}

  subscribe(f: (s: ClientState) => void): () => void {
    this.listeners.add(f);
    f(this.state);
    return () => this.listeners.delete(f);
  }

  private emit() {
    this.state = { ...this.state };
    for (const l of this.listeners) l(this.state);
  }

  connect() {
    this.closed = false;
    this.state.connection = 'connecting';
    this.emit();
    const s = this.factory(this.url);
    this.socket = s;
    s.onopen = () => {
      this.retryMs = 250;
      this.lastSeq = 0;
      this.raw({ type: 'hello', name: 'klight-ui', token: this.token });
    };
    s.onmessage = (ev) => {
      if (typeof ev.data !== 'string') return;
      let msg: ServerMessage;
      try {
        msg = JSON.parse(ev.data) as ServerMessage;
      } catch {
        return;
      }
      this.handle(msg);
    };
    s.onclose = () => {
      this.socket = null;
      this.state.connection = 'disconnected';
      this.state.permission = null;
      for (const p of this.pending.values()) p.reject(new CommandFailed({ kind: 'busy' }));
      this.pending.clear();
      this.emit();
      if (!this.closed) {
        this.scheduler(() => this.connect(), this.retryMs);
        this.retryMs = Math.min(this.retryMs * 2, 5000);
      }
    };
    s.onerror = () => {
      /* onclose follows */
    };
  }

  close() {
    this.closed = true;
    this.socket?.close();
  }

  private raw(m: ClientMessage) {
    this.socket?.send(JSON.stringify(m));
  }

  /** Handles one server message (public for tests). */
  handle(msg: ServerMessage) {
    if (msg.type === 'welcome') {
      this.state.connection = 'connected';
      this.state.permission = msg.permission;
      if (this.dmxSubscription.length) this.raw({ type: 'subscribe_dmx', universes: this.dmxSubscription });
      this.emit();
      return;
    }
    // Gap detection: any missing seq means we lost something → resync.
    const seq = msg.seq;
    const gap = this.lastSeq !== 0 && seq !== this.lastSeq + 1 && msg.type !== 'snapshot';
    this.lastSeq = seq;
    if (gap) {
      this.state.resyncs += 1;
      this.raw({ type: 'request_snapshot' });
    }
    switch (msg.type) {
      case 'snapshot':
        this.showSeq = Math.max(this.showSeq, msg.show_seq);
        this.state.show = msg.show;
        this.state.runtime = msg.runtime;
        this.state.status = msg.status;
        break;
      case 'show_changed':
        this.showSeq = Math.max(this.showSeq, msg.show_seq);
        this.state.show = msg.show;
        break;
      case 'runtime':
        this.state.runtime = msg.runtime;
        break;
      case 'status':
        this.state.status = msg.status;
        break;
      case 'dmx':
        this.state.dmx.set(msg.universe, msg.data);
        break;
      case 'ack': {
        const p = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (!p) return;
        // Resolve only once our show includes the change: callers can then
        // compute the next id or address from an up-to-date show.
        this.whenShowAt(msg.show_seq, () => p.resolve(msg.warnings));
        return;
      }
      case 'error': {
        if (msg.id !== null) {
          const p = this.pending.get(msg.id);
          this.pending.delete(msg.id);
          p?.reject(new CommandFailed(msg.error));
        }
        return;
      }
      case 'learned': {
        const w = this.learnWaiters.shift();
        w?.({ device: msg.device, message: msg.message });
        return;
      }
      default:
        return;
    }
    this.emit();
    this.flushAwaiting();
  }

  private whenShowAt(seq: number, resolve: () => void) {
    if (seq <= this.showSeq) {
      resolve();
      return;
    }
    const entry = { seq, resolve };
    this.awaitingShow.push(entry);
    // Safety net: never hang a caller if an update is lost.
    this.scheduler(() => {
      const i = this.awaitingShow.indexOf(entry);
      if (i >= 0) {
        this.awaitingShow.splice(i, 1);
        resolve();
      }
    }, 2000);
  }

  private flushAwaiting() {
    if (!this.awaitingShow.length) return;
    const ready = this.awaitingShow.filter((a) => a.seq <= this.showSeq);
    this.awaitingShow = this.awaitingShow.filter((a) => a.seq > this.showSeq);
    for (const a of ready) a.resolve();
  }

  /** Sends a command; resolves with warnings, rejects with a typed error. */
  command(command: Command): Promise<string[]> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      if (!this.socket || this.state.connection !== 'connected') {
        reject(new CommandFailed({ kind: 'busy' }));
        return;
      }
      this.pending.set(id, { resolve, reject });
      this.raw({ type: 'command', id, command });
    });
  }

  /**
   * Streams the DMX output of some universes until the returned function is
   * called. Several components can watch at the same time (monitor, scene
   * preview, channel desk): the engine receives the union.
   */
  watchDmx(universes: number[]): () => void {
    for (const u of universes) this.dmxWatchers.set(u, (this.dmxWatchers.get(u) ?? 0) + 1);
    this.sendDmxSubscription();
    let done = false;
    return () => {
      if (done) return;
      done = true;
      for (const u of universes) {
        const n = (this.dmxWatchers.get(u) ?? 1) - 1;
        if (n <= 0) this.dmxWatchers.delete(u);
        else this.dmxWatchers.set(u, n);
      }
      this.sendDmxSubscription();
    };
  }

  private sendDmxSubscription() {
    this.dmxSubscription = [...this.dmxWatchers.keys()].sort((a, b) => a - b);
    this.raw({ type: 'subscribe_dmx', universes: this.dmxSubscription });
  }

  learn(): Promise<{ device: string; message: InputMessage }> {
    return new Promise((resolve) => {
      this.learnWaiters.push(resolve);
      this.raw({ type: 'midi_learn' });
    });
  }
}

export function defaultUrl(loc: { protocol: string; host: string } | undefined): string {
  // Inside Tauri (or file://) the engine runs locally.
  if (!loc || !loc.host || loc.protocol === 'tauri:' || loc.protocol === 'file:') {
    return 'ws://127.0.0.1:9400/ws';
  }
  return `${loc.protocol === 'https:' ? 'wss' : 'ws'}://${loc.host}/ws`;
}
