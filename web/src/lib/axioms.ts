/** Lean's standard axioms. A proof using only these is kernel-verified at level `verified`. */
export const STANDARD_AXIOMS: readonly string[] = ['propext', 'Classical.choice', 'Quot.sound'];

export function isStandardAxiom(axiom: string): boolean {
  return STANDARD_AXIOMS.includes(axiom.trim());
}

export function nonStandardAxioms(axioms: readonly string[]): string[] {
  return axioms.filter((axiom) => !isStandardAxiom(axiom));
}
