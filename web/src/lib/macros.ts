/** Preamble math macros of a paper, in KaTeX syntax: `{"\\rep": "\\operatorname{rep}"}`. */
export type Macros = Record<string, string>;

/**
 * Macros as the server sent them, or `{}` when missing or null. Only
 * `\name` keys with string definitions are kept.
 */
export function normaliseMacros(value: unknown): Macros {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  const out: Macros = {};
  for (const [key, definition] of Object.entries(value as Record<string, unknown>)) {
    if (/^\\(?:[A-Za-z@]+|[^A-Za-z@])$/.test(key) && typeof definition === 'string')
      out[key] = definition;
  }
  return out;
}

/** The macros of the current (last) version of a paper. */
export function currentMacros(versions: readonly { macros?: unknown }[]): Macros {
  return normaliseMacros(versions[versions.length - 1]?.macros);
}
