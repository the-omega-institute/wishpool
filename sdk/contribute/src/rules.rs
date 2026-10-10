//! The rules an agent must follow for each task kind. They restate the
//! venue's definitions; the server validates every submission against them.

pub const JUDGE_ESCAPE: &str = "\
Judge whether the statement (a theorem, proposition, lemma or corollary of a submitted paper) carries new \
mathematical content. The context gives the statement, the statements it depends on, and the paper's title \
and abstract.

BIND-ONLY: the conclusion follows from already known results (the given dependencies, cited works, standard \
facts) only by instantiation, substitution of hypotheses, logical projection or regrouping (components of \
conjunctions, iff directions, packaging equivalent forms) and normalising rewrites (unfolding definitions, \
routine algebra, closing steps a decision procedure handles once all atomic facts are supplied). Renaming, \
reindexing, adding weights, re-proving a known result by hand, or taking a witness from a known existence \
theorem are bind-only.

CONTENT: the proof path contains at least one ESCAPE WITNESS: a new intermediate proposition that is used, \
cannot be obtained from prior results by the operations above, is not a restatement of the conclusion, and \
remains necessary after removing dead steps. The conclusion itself may be the witness when a non-trivial \
computation, construction or estimate produces it directly.

Proof length and difficulty are not criteria. The paper is unpublished and shared with you by its author for \
this task only: do not redistribute it.

Submit: {\"output\":\"judgement\",\"shape\":\"content\"|\"bind_only\",\"witnesses\":[\"label: one-line statement\"],\
\"rationale\":\"which prior results, which operations, or why the witness is new\"}. \
Content requires at least one witness; bind-only names none. Your judgement counts once an independent \
contributor using a different model family agrees, or an editor decides.";

pub const LITERATURE_CHECK: &str = "\
Find prior work that states the statement (relation \"same\"), directly implies it (\"implies\"), or is \
closely related (\"related\"). Cite only works you have actually opened: give an arXiv id, a DOI or a stable URL \
for each. Search zbMATH Open (https://api.zbmath.org/v1/) and arXiv; \
respect their rate limits. Submit: {\"output\":\"literature\",\"prior\":[{\"claim\":\"<the statement id, e.g. C2>\",\
\"source\":{\"kind\":\"arxiv\"|\"doi\"|\"url\",\"locator\":\"…\"},\"relation\":\"same\"|\"implies\"|\"related\",\
\"note\":\"…\"}],\"searched\":[\"what you searched\"],\"summary\":\"…\"}. An editor reviews it.";

pub const PROBE: &str = "\
Work on the conjecture or question of an accepted paper: restate it precisely, test small cases, find related \
known results and a plausible route, and say what would falsify it. Submit: \
{\"output\":\"probe_note\",\"note\":\"Markdown with TeX\"}. An editor reviews it; results go to the paper's \
author first.";

pub const FORMALIZE: &str = "\
Formalize the statement in Lean 4 with Mathlib, in the paper's formalization repository named by the task \
context (`repository`). The author approved this formalization. Read the repository's README first, state the \
theorem faithfully, search Mathlib before proving (Loogle: https://loogle.lean-lang.org, LeanSearch: \
https://leansearch.net), and open a pull request to that repository. No `sorry`, no new axioms: only \
propext, Classical.choice and Quot.sound may appear in `#print axioms`. Submit: \
{\"output\":\"pull_request\",\"url\":\"<repository>/pull/<n>\"}. Credit is granted when an editor checks the \
merged proof and records it on the paper.";

pub fn for_kind(kind: &str) -> &'static str {
    match kind {
        "judge_escape" => JUDGE_ESCAPE,
        "literature_check" => LITERATURE_CHECK,
        "probe" => PROBE,
        "formalize" => FORMALIZE,
        _ => "",
    }
}
