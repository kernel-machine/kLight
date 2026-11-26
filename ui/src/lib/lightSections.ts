// Fixtures arranged to be found at a glance: one section per model, both the
// sections and their fixtures in natural order.
import type { PatchedFixture } from './protocol';

export interface LightSection {
  /** The fixtures' definition. */
  key: string;
  /** The model; with the manufacturer when two models share a name. */
  title: string;
  fixtures: PatchedFixture[];
}

const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

/** Whether `text` contains the search `query`, ignoring case. */
export function found(text: string, query: string): boolean {
  return text.toLowerCase().includes(query.trim().toLowerCase());
}

/** Sections of the fixtures whose name or model matches `query` (empty: all). */
export function lightSections(fixtures: PatchedFixture[], query = ''): LightSection[] {
  const model = (definition: string) => definition.slice(definition.indexOf('/') + 1);
  const definitions = [...new Set(fixtures.map((f) => f.definition))];
  return definitions
    .map((key) => ({
      key,
      title: definitions.some((d) => d !== key && model(d) === model(key)) ? key.replace('/', ' ') : model(key),
      fixtures: fixtures
        .filter((f) => f.definition === key && (found(f.name, query) || found(f.definition, query)))
        .sort((a, b) => byName.compare(a.name, b.name)),
    }))
    .filter((s) => s.fixtures.length > 0)
    .sort((a, b) => byName.compare(a.title, b.title));
}
