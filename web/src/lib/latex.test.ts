import { describe, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { LatexText } from '../components/Markdown';
import { latexToMarkdown, splitMath, stripComments } from './latex';

describe('latexToMarkdown', () => {
  it('keeps inline math and turns \\( \\) into $', () => {
    expect(latexToMarkdown('Let $n \\ge 1$ and \\(x>0\\).')).toBe('Let $n \\ge 1$ and $x>0$.');
  });

  it('turns display math and environments into $$ blocks', () => {
    expect(latexToMarkdown('Then \\[ a = b \\label{eq:1} \\] holds.')).toBe(
      'Then\n\n$$\na = b\n$$\n\nholds.',
    );
    expect(latexToMarkdown('We have\n\\begin{align*} a &= b \\\\ c &= d \\end{align*}')).toBe(
      'We have\n\n$$\n\\begin{align*} a &= b \\\\ c &= d \\end{align*}\n$$',
    );
    expect(latexToMarkdown('\\begin{equation}\\label{x} e^{i\\pi}+1=0 \\end{equation}')).toBe(
      '$$\ne^{i\\pi}+1=0\n$$',
    );
    expect(latexToMarkdown('\\begin{eqnarray} a&=&b \\end{eqnarray}')).toBe(
      '$$\n\\begin{align} a&=&b \\end{align}\n$$',
    );
  });

  it('rewrites text commands to Markdown', () => {
    expect(latexToMarkdown('\\emph{every} \\textbf{prime} \\texttt{p}')).toBe(
      '*every* **prime** `p`',
    );
    expect(latexToMarkdown('By~\\cite[Thm.~2]{smith2020, jones} and Lemma~\\ref{lem:a}')).toBe(
      'By \\[smith2020, jones, Thm. 2\\] and Lemma lem:a',
    );
    expect(latexToMarkdown('see \\href{https://oeis.org/A1}{OEIS} and \\href{ftp://x}{this}')).toBe(
      'see [OEIS](https://oeis.org/A1) and this',
    );
  });

  it('unwraps the nested text styles around math in arXiv 2609.25128', () => {
    const source = String.raw`\textnormal{\textsc{Sub-Quorum-$K$}} is NP-complete, even for bipartite graphs.`;
    expect(latexToMarkdown(source)).toBe(
      'Sub-Quorum-$K$ is NP-complete, even for bipartite graphs.',
    );
    const rendered = document.createElement('div');
    rendered.innerHTML = renderToStaticMarkup(createElement(LatexText, { source }));
    expect(rendered.querySelector('.katex-mathml annotation')?.textContent).toBe('K');
    expect(rendered.textContent).toContain('Sub-Quorum-');
    expect(rendered.textContent).not.toMatch(/\\textnormal|\\textsc|[{}]/);
  });

  it.each(['textnormal', 'textsc', 'textrm', 'textup', 'textsf', 'texttt'])(
    'unwraps nested %s while retaining math and emphasis',
    (command) => {
      expect(latexToMarkdown(`\\${command}{\\textsc{Sub-Quorum-$K$}}`)).toBe('Sub-Quorum-$K$');
      expect(latexToMarkdown(`\\${command}{\\emph{every $x$} and \\textbf{some $y$}}`)).toBe(
        '*every $x$* and **some $y$**',
      );
    },
  );

  it.each([
    ['emph', '*'],
    ['textit', '*'],
    ['textbf', '**'],
  ])('keeps %s around a complete argument containing math', (command, marker) => {
    expect(latexToMarkdown(`\\${command}{\\textnormal{every $x^{2}$} is positive}`)).toBe(
      `${marker}every $x^{2}$ is positive${marker}`,
    );
    expect(latexToMarkdown(`\\${command}{\\(x\\) and $y$}`)).toBe(`${marker}$x$ and $y$${marker}`);
  });

  it('keeps emphasis in the rendered output when its argument contains math', () => {
    const source = String.raw`\emph{every $x$} and \textbf{some $y$}`;
    const rendered = document.createElement('div');
    rendered.innerHTML = renderToStaticMarkup(createElement(LatexText, { source }));
    expect(rendered.querySelector('em .katex')).not.toBeNull();
    expect(rendered.querySelector('strong .katex')).not.toBeNull();
  });

  it('renders grouped citations in statement titles without stray braces', () => {
    const source = String.raw`Equivalent form of the grid $3$-path-cover formula {\cite{Bresar2013,JakovacTaranenko2013}}`;
    expect(latexToMarkdown(source)).toBe(
      'Equivalent form of the grid $3$-path-cover formula \\[Bresar2013, JakovacTaranenko2013\\]',
    );
  });

  it('drops comments and labels but keeps escaped characters', () => {
    expect(stripComments('a % comment\nb \\% c')).toBe('a \nb \\% c');
    expect(latexToMarkdown('\\label{thm:x}50\\% of $n$ % note')).toBe('50% of $n$');
    expect(latexToMarkdown('costs \\$5 and $x$')).toBe('costs \\$5 and $x$');
  });

  it('turns itemize into a Markdown list', () => {
    expect(latexToMarkdown('\\begin{enumerate}\\item[(i)] $a$;\\item $b$.\\end{enumerate}')).toBe(
      '- (i) $a$;\n- $b$.',
    );
  });

  it('escapes Markdown syntax in text and leaves unknown commands visible', () => {
    expect(latexToMarkdown('a_b * c')).toBe('a\\_b \\* c');
    expect(latexToMarkdown('\\foo bar')).toBe('\\foo bar');
  });

  it('splits math without treating an escaped dollar as a delimiter', () => {
    expect(splitMath('\\$1 and $x$')).toEqual([
      { kind: 'text', text: '\\$1 and ' },
      { kind: 'math', display: false, tex: 'x' },
    ]);
  });
});
