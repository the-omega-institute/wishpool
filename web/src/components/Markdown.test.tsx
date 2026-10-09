import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { currentMacros, normaliseMacros } from '../lib/macros';
import { KATEX_ERROR, MACROS } from '../test/katex';
import { LatexText, MacrosProvider } from './Markdown';

describe('paper macros in KaTeX', () => {
  it('renders a preamble macro as an operator name instead of an error', () => {
    const { container } = render(
      <MacrosProvider macros={MACROS}>
        <LatexText source="Let $\rep(x) = \code{ab}$." />
      </MacrosProvider>,
    );
    expect(container.querySelector('.katex')).not.toBeNull();
    expect(container.querySelector(KATEX_ERROR)).toBeNull();
    expect(container.querySelector('.mop')).toHaveTextContent('rep');
    expect(container.querySelector('.texttt')).toHaveTextContent('ab');
  });

  it('shows an unknown command as an error without the macros', () => {
    const { container } = render(<LatexText source="Let $\rep(x)$." />);
    expect(container.querySelector(KATEX_ERROR)).not.toBeNull();
  });

  it('gives each render its own copy, so \\gdef does not leak', () => {
    const shared = { ...MACROS };
    const { container } = render(
      <MacrosProvider macros={shared}>
        <LatexText source="$\gdef\leak{x}\leak$" />
        <div className="second">
          <LatexText source="$\leak$" />
        </div>
      </MacrosProvider>,
    );
    expect(shared).toEqual(MACROS);
    expect(container.querySelector('.second')?.querySelector(KATEX_ERROR)).not.toBeNull();
  });

  it('treats missing or null macros as none', () => {
    expect(normaliseMacros(null)).toEqual({});
    expect(normaliseMacros(undefined)).toEqual({});
    expect(normaliseMacros({ '\\R': '\\mathbb{R}', bad: 'x', '\\n': 3 })).toEqual({
      '\\R': '\\mathbb{R}',
    });
    expect(currentMacros([{ macros: { '\\a': '1' } }, { macros: null }])).toEqual({});
    expect(currentMacros([{ macros: { '\\a': '1' } }, { macros: { '\\b': '2' } }])).toEqual({
      '\\b': '2',
    });
    expect(currentMacros([])).toEqual({});
  });
});
