/**
 * KaTeX marks a formula it cannot parse with `.katex-error`, and an undefined
 * command (with `throwOnError: false`) as red text; jsdom normalises the colour.
 */
export const KATEX_ERROR =
  '.katex-error, [style*="color:#cc0000"], [style*="color: rgb(204, 0, 0)"]';

/** Paper macros used across the tests. */
export const MACROS = { '\\rep': '\\operatorname{rep}', '\\code': '\\texttt{#1}' };
