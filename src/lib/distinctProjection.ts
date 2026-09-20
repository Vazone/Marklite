import { get, readable, type Readable } from 'svelte/store';

/**
 * Projects a store while suppressing updates whose observable value did not
 * change. This keeps large source objects out of consumers that only need a
 * small descriptor view.
 */
export function distinctProjection<Source, Value>(
  source: Readable<Source>,
  project: (source: Source) => Value,
  equals: (left: Value, right: Value) => boolean = Object.is
): Readable<Value> {
  let current = project(get(source));
  return readable(current, (set) =>
    source.subscribe((value) => {
      const next = project(value);
      if (equals(current, next)) return;
      current = next;
      set(next);
    })
  );
}
