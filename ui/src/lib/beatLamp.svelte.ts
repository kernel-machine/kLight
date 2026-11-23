// A reactive "on the beat" flag for components (call during component init).

import { currentBeatFlash } from './beat';

/** `enabled` can change over time; the animation loop runs only while it is true. */
export function createBeatLamp(enabled: () => boolean = () => true) {
  const lamp = $state({ on: false, down: false });
  $effect(() => {
    if (!enabled()) {
      lamp.on = false;
      lamp.down = false;
      return;
    }
    let raf = 0;
    const tick = () => {
      const f = currentBeatFlash();
      if (lamp.on !== f.on) lamp.on = f.on;
      if (lamp.down !== f.down) lamp.down = f.down;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });
  return lamp;
}
