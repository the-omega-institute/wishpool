import { describe, expect, it } from 'vitest';
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
