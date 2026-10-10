/*
 * Statements arrive as LaTeX source read from the author's paper. They are
 * shown through the Markdown renderer, whose math plugins hand `$…$` and
 * `$$…$$` to KaTeX. This module rewrites the LaTeX into that form: math
 * delimiters and display environments become `$`/`$$`, common text
 * commands become Markdown, and labels and comments are dropped. Commands
 * it does not know stay visible as source.
 */

const DISPLAY_ENVS = new Set([
  'equation',
  'equation*',
  'align',
  'align*',
  'alignat',
  'alignat*',
  'gather',
  'gather*',
  'multline',
  'multline*',
  'flalign',
  'flalign*',
  'eqnarray',
  'eqnarray*',
  'displaymath',
  'math',
]);

/** Environments KaTeX knows; the others are rewritten to one it knows. */
const ENV_REWRITE: { [env: string]: string } = {
  multline: 'gather',
  'multline*': 'gather*',
  flalign: 'align',
  'flalign*': 'align*',
  eqnarray: 'align',
  'eqnarray*': 'align*',
};

type Segment = { kind: 'text'; text: string } | { kind: 'math'; display: boolean; tex: string };

/** Read a complete math span without splitting any surrounding text-command argument. */
function readMath(
  src: string,
  start: number,
): { segment: Extract<Segment, { kind: 'math' }>; end: number } | null {
  const rest = src.slice(start);
  const delims: [string, string, boolean][] = [
    ['$$', '$$', true],
    ['\\[', '\\]', true],
    ['\\(', '\\)', false],
    ['$', '$', false],
  ];
  const delim = delims.find(([open]) => rest.startsWith(open));
  if (delim) {
    const [open, close, display] = delim;
    const end = findClose(src, start + open.length, close);
    if (end >= 0) {
      return {
        segment: { kind: 'math', display, tex: src.slice(start + open.length, end) },
        end: end + close.length,
      };
    }
  }
  const env = /^\\begin\{([A-Za-z*]+)\}/.exec(rest);
  if (env && env[1] !== undefined && DISPLAY_ENVS.has(env[1])) {
    const name = env[1];
    const closeTag = `\\end{${name}}`;
    const end = src.indexOf(closeTag, start + env[0].length);
    if (end >= 0) {
      const body = src.slice(start + env[0].length, end);
      const tex =
        name === 'equation' || name === 'equation*' || name === 'displaymath' || name === 'math'
          ? body
          : `\\begin{${ENV_REWRITE[name] ?? name}}${body}\\end{${ENV_REWRITE[name] ?? name}}`;
      return {
        segment: { kind: 'math', display: name !== 'math', tex },
        end: end + closeTag.length,
      };
    }
  }
  return null;
}

/** Drop `%` comments (an unescaped `%` to the end of the line). */
export function stripComments(src: string): string {
  return src
    .split('\n')
    .map((line) => {
      for (let i = 0; i < line.length; i += 1) {
        if (line[i] === '\\') {
          i += 1;
          continue;
        }
        if (line[i] === '%') return line.slice(0, i);
      }
      return line;
    })
    .join('\n');
}

/** The index just past the brace group starting at `open` (which must be `{`), or -1. */
function closeBrace(src: string, open: number): number {
  let depth = 0;
  for (let i = open; i < src.length; i += 1) {
    const c = src[i];
    if (c === '\\') {
      i += 1;
      continue;
    }
    if (c === '{') depth += 1;
    else if (c === '}') {
      depth -= 1;
      if (depth === 0) return i + 1;
    }
  }
  return -1;
}

/** Split LaTeX into text and math, recognising `$`, `$$`, `\(`, `\[` and display environments. */
export function splitMath(src: string): Segment[] {
  const out: Segment[] = [];
  let text = '';
  const flush = () => {
    if (text !== '') out.push({ kind: 'text', text });
    text = '';
  };
  let i = 0;
  while (i < src.length) {
    const rest = src.slice(i);
    if (rest.startsWith('\\$')) {
      text += '\\$';
      i += 2;
      continue;
    }
    const math = readMath(src, i);
    if (math) {
      flush();
      out.push(math.segment);
      i = math.end;
      continue;
    }
    text += src[i];
    i += 1;
  }
  flush();
  return out;
}

/** The start of the closing delimiter, skipping escaped characters; -1 if absent. */
function findClose(src: string, from: number, close: string): number {
  for (let i = from; i < src.length; i += 1) {
    if (src.startsWith(close, i)) {
      // `$` must not close on an escaped `\$`; `\)` and `\]` are themselves escapes.
      if (close.startsWith('$') && src[i - 1] === '\\') continue;
      return i;
    }
    if (src[i] === '\\' && !close.startsWith('\\')) i += 1;
  }
  return -1;
}

const TEX_IN_MATH: [RegExp, string][] = [
  [/\\label\{[^{}]*\}/g, ''],
  [/\\nonumber\b|\\notag\b/g, ''],
];

function cleanMath(tex: string): string {
  let out = tex;
  for (const [pattern, replacement] of TEX_IN_MATH) out = out.replace(pattern, replacement);
  return out.trim();
}

/** Escape characters Markdown would read as syntax in plain text. */
function escapeMarkdown(text: string): string {
  return text.replace(/([*_`[\]<>#|])/g, '\\$1');
}

const WRAPPERS: { [command: string]: [string, string] } = {
  emph: ['*', '*'],
  textit: ['*', '*'],
  textsl: ['*', '*'],
  textbf: ['**', '**'],
  textsc: ['', ''],
  textrm: ['', ''],
  textsf: ['', ''],
  textup: ['', ''],
  textnormal: ['', ''],
  mbox: ['', ''],
  text: ['', ''],
  underline: ['', ''],
};

/** Parse text-command groups before rendering the math spans inside them. */
function fragmentToMarkdown(src: string): string {
  let out = '';
  let i = 0;
  while (i < src.length) {
    const c = src[i] as string;
    const math = c === '$' || c === '\\' ? readMath(src, i) : null;
    if (math) {
      const { display, tex } = math.segment;
      out += display ? `\n\n$$\n${cleanMath(tex)}\n$$\n\n` : `$${cleanMath(tex)}$`;
      i = math.end;
      continue;
    }
    if (c === '\\') {
      const cmd = /^\\([A-Za-z]+)\*?\s*/.exec(src.slice(i));
      if (cmd && cmd[1] !== undefined) {
        const name = cmd[1];
        let j = i + cmd[0].length;
        // Optional argument, e.g. \cite[Thm. 2]{key}.
        let optional = '';
        if (src[j] === '[') {
          const end = src.indexOf(']', j);
          if (end > j) {
            optional = src.slice(j + 1, end).replace(/~/g, ' ');
            j = end + 1;
          }
        }
        const readArg = (): string | null => {
          if (src[j] !== '{') return null;
          const end = closeBrace(src, j);
          if (end < 0) return null;
          const arg = src.slice(j + 1, end - 1);
          j = end;
          return arg;
        };
        if (name === 'label' || name === 'index') {
          if (readArg() !== null) {
            i = j;
            continue;
          }
        }
        const wrapper = WRAPPERS[name];
        if (wrapper) {
          const arg = readArg();
          if (arg !== null) {
            out += `${wrapper[0]}${fragmentToMarkdown(arg)}${wrapper[1]}`;
            i = j;
            continue;
          }
        }
        if (name === 'texttt' || name === 'url') {
          const arg = readArg();
          if (arg !== null) {
            // Markdown code spans cannot contain rendered math or emphasis.
            // Unwrap styled/mathematical texttt arguments so both still render.
            out += name === 'texttt' && /[\\$]/.test(arg) ? fragmentToMarkdown(arg) : `\`${arg}\``;
            i = j;
            continue;
          }
        }
        if (name === 'href') {
          const url = readArg();
          const label = url === null ? null : readArg();
          if (url !== null && label !== null) {
            out += /^https?:\/\//.test(url)
              ? `[${fragmentToMarkdown(label)}](${url})`
              : fragmentToMarkdown(label);
            i = j;
            continue;
          }
        }
        if (name === 'cite' || name === 'citep' || name === 'citet') {
          const keys = readArg();
          if (keys !== null) {
            const list = keys
              .split(',')
              .map((k) => k.trim())
              .join(', ');
            out += `\\[${escapeMarkdown(list)}${optional ? `, ${escapeMarkdown(optional)}` : ''}\\]`;
            i = j;
            continue;
          }
        }
        if (
          name === 'ref' ||
          name === 'eqref' ||
          name === 'autoref' ||
          name === 'cref' ||
          name === 'Cref'
        ) {
          const key = readArg();
          if (key !== null) {
            out += name === 'eqref' ? `(${escapeMarkdown(key)})` : escapeMarkdown(key);
            i = j;
            continue;
          }
        }
        if (name === 'item') {
          out += `\n- ${optional ? `${escapeMarkdown(optional)} ` : ''}`;
          while (src[j] === ' ' || src[j] === '\t' || src[j] === '\n') j += 1;
          i = j;
          continue;
        }
        if (name === 'begin' || name === 'end') {
          const env = readArg();
          if (env !== null) {
            // Lists become Markdown lists; other environments keep only their body.
            out += '\n';
            i = j;
            continue;
          }
        }
        if (
          [
            'noindent',
            'medskip',
            'smallskip',
            'bigskip',
            'par',
            'newline',
            'centering',
            'quad',
            'qquad',
          ].includes(name)
        ) {
          out += name === 'par' || name === 'newline' ? '\n\n' : ' ';
          i = j;
          continue;
        }
        if (name === 'ldots' || name === 'dots') {
          out += '…';
          i = i + cmd[0].length;
          continue;
        }
        // Unknown command: keep it visible.
        out += escapeMarkdown(src.slice(i, i + cmd[0].length));
        i += cmd[0].length;
        continue;
      }
      const next = src[i + 1];
      if (next === '\\') {
        out += '  \n';
        i += 2;
        continue;
      }
      if (next !== undefined && '%&#_{}$'.includes(next)) {
        out += next === '$' ? '\\$' : escapeMarkdown(next);
        i += 2;
        continue;
      }
      if (next === ',' || next === ' ' || next === ';') {
        out += ' ';
        i += 2;
        continue;
      }
      out += '\\\\';
      i += 1;
      continue;
    }
    if (src.startsWith('---', i)) {
      out += '—';
      i += 3;
      continue;
    }
    if (src.startsWith('--', i)) {
      out += '–';
      i += 2;
      continue;
    }
    if (src.startsWith('``', i)) {
      out += '“';
      i += 2;
      continue;
    }
    if (src.startsWith("''", i)) {
      out += '”';
      i += 2;
      continue;
    }
    if (c === '~') {
      out += ' ';
      i += 1;
      continue;
    }
    if (c === '{' || c === '}') {
      i += 1;
      continue;
    }
    out += escapeMarkdown(c);
    i += 1;
  }
  return out;
}

/** Rewrite a LaTeX statement into Markdown with `$…$` / `$$…$$` math for KaTeX. */
export function latexToMarkdown(src: string): string {
  const out = fragmentToMarkdown(stripComments(src));
  return out
    .split('\n')
    .map((line) =>
      line.replace(/[ \t]+$/g, (m) => (m.length >= 2 ? '  ' : '')).replace(/^[ \t]+/, ''),
    )
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}
