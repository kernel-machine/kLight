// Switching the pattern of a matrix keeps the colors the person chose: the
// main color goes to the main color of the new pattern and the second one
// (background, or the other end of a gradient) to its second. Editor
// convenience only: what the effect does with them is the core's.
import type { Color, MatrixAlgorithm } from './protocol';

type Colors = { main?: Color; second?: Color };

const colorsOf = (a: MatrixAlgorithm): Colors => {
  const o = a as Record<string, unknown>;
  const main = (o.color ?? o.from) as Color | undefined;
  const second = (o.background ?? o.to) as Color | undefined;
  return { main, second };
};

/** `next` (a pattern with its default colors) wearing the colors of `old`. */
export function carryColors(old: MatrixAlgorithm, next: MatrixAlgorithm): MatrixAlgorithm {
  const { main, second } = colorsOf(old);
  const o = { ...next } as Record<string, unknown>;
  const mainKey = 'color' in o ? 'color' : 'from' in o ? 'from' : null;
  const secondKey = 'background' in o ? 'background' : 'to' in o ? 'to' : null;
  if (mainKey && main) o[mainKey] = { ...main };
  if (secondKey && second) o[secondKey] = { ...second };
  return o as MatrixAlgorithm;
}
