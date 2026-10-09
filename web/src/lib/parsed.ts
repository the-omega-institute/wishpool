/** The result of reading a form value: the value, or a message for the person filling it in. */
export type Parsed<T> = { ok: true; value: T } | { ok: false; error: string };

export function ok<T>(value: T): Parsed<T> {
  return { ok: true, value };
}

export function fail<T>(error: string): Parsed<T> {
  return { ok: false, error };
}

/** Non-empty trimmed lines of a textarea. */
export function lines(text: string): string[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
}

/** A whole number ≥ 0 typed into a form; empty gives `undefined`. */
export function parseCount(text: string): Parsed<number | undefined> {
  const cleaned = text.replace(/[\s,_]/g, '');
  if (cleaned === '') return ok(undefined);
  if (!/^\d+$/.test(cleaned)) return fail('Token counts are whole numbers.');
  return ok(Number(cleaned));
}
