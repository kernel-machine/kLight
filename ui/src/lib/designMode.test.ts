import { describe, expect, it, vi } from 'vitest';
import { CommandFailed } from './client';
import { enterDesignMode } from './designMode';
import type { Command } from './protocol';

const badPin = () => new CommandFailed({ kind: 'command', error: { type: 'bad_pin' } });

/** An engine that wants `pin`, or none at all. */
function engine(pin: string | null) {
  return vi.fn(async (c: Command) => {
    if (c.type === 'set_mode' && pin !== null && c.pin !== pin) throw badPin();
  });
}

describe('entering Design mode', () => {
  it('asks nothing when the engine needs no PIN', async () => {
    const send = engine(null);
    const ask = vi.fn(() => '1234');
    await enterDesignMode(send, ask);
    expect(ask).not.toHaveBeenCalled();
    expect(send.mock.calls).toEqual([[{ type: 'set_mode', mode: 'Design', pin: null }]]);
  });

  it('asks for the PIN the engine needs', async () => {
    const send = engine('1234');
    await enterDesignMode(send, () => '1234');
    expect(send).toHaveBeenLastCalledWith({ type: 'set_mode', mode: 'Design', pin: '1234' });
    await expect(enterDesignMode(send, () => '0000')).rejects.toThrow('Wrong PIN');
  });

  it('gives up quietly when the question is cancelled', async () => {
    const send = engine('1234');
    await enterDesignMode(send, () => null);
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('reports any other failure without asking', async () => {
    const ask = vi.fn(() => '1234');
    const busy = new CommandFailed({ kind: 'busy' });
    await expect(enterDesignMode(() => Promise.reject(busy), ask)).rejects.toBe(busy);
    expect(ask).not.toHaveBeenCalled();
  });
});
