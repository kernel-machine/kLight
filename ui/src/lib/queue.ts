// Edits applied one after the other, each on the result of the previous one.

/**
 * Returns a function that runs a task after the ones given before it. The
 * promise resolves when the task is done; a task that fails is reported to
 * `onError` and does not stop the ones after it.
 */
export function editQueue(onError: (error: unknown) => void): (task: () => unknown) => Promise<void> {
  let tail: Promise<void> = Promise.resolve();
  return (task) => {
    tail = tail
      .then(async () => {
        await task();
      })
      .catch((error: unknown) => {
        onError(error);
      });
    return tail;
  };
}
