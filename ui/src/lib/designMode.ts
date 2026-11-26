// Switching to Design mode. Whether a PIN is needed is the engine's rule (a
// remote client and a show with a PIN): it is asked only when the engine
// refuses to switch without one.
import { CommandFailed } from './client';
import type { Command } from './protocol';

export async function enterDesignMode(send: (command: Command) => Promise<unknown>, askPin: () => string | null): Promise<void> {
  try {
    await send({ type: 'set_mode', mode: 'Design', pin: null });
  } catch (e) {
    const needsPin = e instanceof CommandFailed && e.error.kind === 'command' && e.error.error.type === 'bad_pin';
    if (!needsPin) throw e;
    const pin = askPin();
    // Cancelled: the mode stays as it is.
    if (pin) await send({ type: 'set_mode', mode: 'Design', pin });
  }
}
