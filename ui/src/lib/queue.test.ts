import { describe, expect, it } from 'vitest';
import { editQueue } from './queue';

describe('editQueue', () => {
  it('runs each task after the previous one has finished', async () => {
    const queue = editQueue(() => {});
    const log: string[] = [];
    let release!: () => void;
    const first = queue(() => new Promise<void>((r) => (release = r)).then(() => log.push('first')));
    const second = queue(() => log.push('second'));
    await Promise.resolve();
    expect(log).toEqual([]);
    release();
    await Promise.all([first, second]);
    expect(log).toEqual(['first', 'second']);
  });

  it('reports a failing task and keeps running the next ones', async () => {
    const errors: unknown[] = [];
    const queue = editQueue((e) => errors.push(e));
    const log: string[] = [];
    const boom = new Error('boom');
    const failing = queue(() => {
      throw boom;
    });
    const next = queue(() => log.push('next'));
    await expect(failing).resolves.toBeUndefined();
    await next;
    expect(log).toEqual(['next']);
    expect(errors).toEqual([boom]);
  });
});
