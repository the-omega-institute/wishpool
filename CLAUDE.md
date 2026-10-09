# wishpool

This file is the contract for anyone — human or agent — working in this
repository. Rules here are binding.

## 1. What wishpool is

A venue where mathematicians submit their own papers as LaTeX source. The
venue reads every statement of the paper, analyses which statements carry new
mathematical content (escape analysis), publishes the papers that pass the
threshold with a public record, and, with the author's consent, formalizes
the valuable statements in Lean.

- **Papers**: the author uploads a `.tex`, `.zip` or `.tar.gz` source. The
  venue reads the title, authors, abstract and every theorem-like
  environment, compiles the PDF with TeX Live, and the author confirms the
  statements (kind, main result or supporting, dependencies).
- **Analysis**: per statement, whether the literature already states or
  implies it, and whether its proof carries an escape witness (a new
  intermediate proposition that prior results do not give by instantiation,
  projection or normalisation) or is bind-only.
- **Threshold and records**: accepted papers receive `WP-<year>-NNNN` and a
  public page. The author decides whether the analysis is public. Papers that
  do not pass keep a private report and may be revised.
- **After acceptance**: editors propose statements to formalize; the author
  approves each; verified Lean proofs appear on the public page. Conjectures
  the paper poses are followed up; results go to the author first.
- **Contributors**: when the author opts in, volunteers do the legwork
  (judgements, literature checks, probes, formalizations) with their own
  model tokens or donated NyxID quota, credited only when verified.

Review stages:

| Code | Stage | Filed by |
|---|---|---|
| S0 | Source: the source is read, the PDF compiles, AI use is disclosed | worker (TeX Live) |
| S1 | Statements and their dependency graph | **the author** |
| S2 | Literature: no main result is already stated or directly implied | **human** (machine leads are proposals) |
| S3 | Escape: bind-only vs content, escape witnesses | **human** (machine and contributor judgements are proposals; an editor adopts them) |

Threshold (`api/crates/layer2-core/src/policy.rs`, the only authority):
accept on **escape_witness** (a main result is judged content and is not
known) or **open_problem_settlement** (a main result settles a named, sourced
open problem the literature had not settled). Every non-acceptance carries a
published reason.

Escape rates are reported only as exact readings on an explicitly built finite
arena. Never compute, estimate or display a percentage for a statement whose
arena was not built and measured.

## 2. Repository layout

The root contains exactly: `.github/ api/ web/ sdk/ infra/ docs/ CLAUDE.md
README.md .gitignore .dockerignore`. Tooling config lives inside `api/`,
`web/` or the SDK package it governs. `sdk/` holds self-contained client
packages (each with its own manifest and lockfile). CI rejects anything else.

## 3. Layers

```
api/crates/
  layer1-public/   Layer 1: REST projection of Layer 2. No business rules.
  layer2-core/     Layer 2: domain, threshold policy, judgements and analysis,
                   formalization, contribution network, ports, services. No I/O.
  layer3-latex/    Layer 3: unpack, read and compile LaTeX sources.
  layer3-review/   Layer 3: models via the NyxID gateway, OpenAlex.
  wishpool/        binary: composition, MongoDB (GridFS for files), NyxID
                   sign-in and delegated tokens, workers.
sdk/contribute/    the contributors' CLI and MCP server.
```

- Layer 1 depends only on Layer 2. Layer 2 depends on no workspace crate and
  no I/O crate. Layer 3 crates depend on neither Layer 1 nor Layer 2. CI
  enforces it.
- Authorization is decided in Layer 2, never in a route or the frontend.
- Every interface is a projection: the web app calls no route a third-party
  client could not call.
- Provider-specific code (a model API, a search engine, a TeX engine) lives in
  Layer 3; the contracts in `layer3-*/src/lib.rs` name none of the domain's
  rules.

## 4. The author owns the paper

- A paper in draft, in review, not accepted or withdrawn is visible only to
  its authors and to staff (editors, reviewer accounts, admins). Respond
  `not_found`, never `forbidden`, to anyone else.
- Contributors see a statement, its dependencies, and the paper's title and
  abstract only while the author keeps `open_to_contributors` on. Turning it
  off closes the paper's open tasks.
- The analysis of an accepted paper is public only after the author chooses
  `public`. Formalization of a statement starts only after the author
  approves it.
- Only the author confirms statements (S1). A new version returns the paper
  to draft; a changed statement set voids S2/S3 reports, judgements and tasks
  of the old set by revision, never by deletion.

## 5. Judgements and credit

- Escape judgements are judgements with provenance. Never present them as
  machine-checked. An editor's judgement confirms; two machine judgements
  from different model families and accounts that agree corroborate;
  disagreement is a dispute for an editor. S3 is filed by an editor.
- A judgement contribution counts when it is corroborated or confirmed;
  literature checks and probes when an editor accepts them; a formalization
  when an editor records the checked Lean proof (repository, full commit,
  declarations, standard axioms only: `propext`, `Classical.choice`,
  `Quot.sound`).
- Token usage sent through the API is stored as self-reported; only usage the
  NyxID gateway reported for hosted work is metered. The two are never summed.
- Delegated refresh tokens are stored sealed (AES-256-GCM) and never logged.

## 6. State

- MongoDB is the only authority; uploaded sources and PDFs live in GridFS.
  Aggregates carry `revision`; every replace is fenced on the revision read.
- Stage reports are append-only. Accepted records are immutable.
- Machine work (compiling, S2 leads, S3 proposals) is a durable, leased job.
  A worker restart must not lose or duplicate admitted work.

## 7. Identity and security

- Sign-in is NyxID OIDC (authorization code + PKCE, nonce, browser-bound
  state). The browser holds only an opaque HttpOnly session cookie; its
  SHA-256 digest is stored. Machine clients present NyxID access tokens,
  verified locally against NyxID's JWKS.
- Cookie-authenticated unsafe requests must come from the public origin.
- Dev sign-in and memory storage exist for local work only and are refused on
  non-loopback binds.
- Uploaded LaTeX is untrusted. Archives are unpacked with limits on size,
  count and paths. TeX runs without shell escape, with paranoid
  `openin_any`/`openout_any`, a cleared environment (no deployment secret is
  visible to it), a fresh work directory and a deadline.
- No secret, token or credential enters the repository, including tests and
  fixtures. Test keys are generated at runtime. `infra/api/secret.yaml` names
  keys and never carries values.
- Model output is untrusted. It is sanitised in the binary's review mapping;
  anything that cannot be represented faithfully is dropped and named in the
  report summary. Never invent a witness, a citation or an identifier;
  literature leads name only works a search engine returned.

## 8. infra never diverges from the code

A change that adds or renames a configuration key, port, health endpoint or
image input updates `infra/` in the same change. The API image serves two
roles: `WISHPOOL_ROLE=api` (stateless replicas) and `worker` (exactly one
replica owning the TeX cache volume). CI checks that every key read in
`config.rs` appears in `infra/api/configmap.yaml` or `secret.yaml`.

## 9. Gates

API: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features --
-D warnings`, `cargo test --workspace` (with `WISHPOOL_TEST_MONGODB_URI` for
the MongoDB tests and `WISHPOOL_TEST_TEX_BIN` for the compile tests),
`cargo deny check`. SDK: the same fmt, clippy and test gates inside each
`sdk/*` package. Web: `npm ci`, `format:check`, `lint`, `typecheck`, `test`,
`build`, `npm audit --audit-level=high`. Cross-cutting: gitleaks, image
builds, kubeconform.

## 10. Branches

`develop` is the default branch; `main` is production and accepts PRs only
from `develop`. No force pushes to either.


## 11. Referee rounds and feedback

- S1 confirmation queues a referee round on the exact paper version and
  statement revision. Settled steps are immutable; editors restart by creating
  a new round after the current round settles. Layer 2 owns ordering, fencing,
  letter validation and all view rules.
- Referee recommendations are advice, never decisions. S2/S3 human adoption and
  `Policy::decide` remain the only publication flow. Referee concerns, citations
  and computational claims are reported findings, never verified evidence.
- Referee statement readings are machine judgements under the separate referee
  account (default `wishpool:referee`, engine `nyxid-oracle`), eligible for the
  existing independent-account, different-model-family corroboration rule.
- Advice runs only for `accept` or `minor_revision`, distinguishes evidence-backed
  reported checks from proposals, and proposes Lean 4 + Mathlib formalization
  only for proved statements. A sketch does not verify a theorem. Actual
  formalization still needs acceptance and the author's approval.
- A round may run a private, staff-only formalization probe on proposed
  statements. The binary itself compiles each Lean file and checks its axioms;
  the prover's account is never the result. A compiled probe is not a recorded
  formalization, and publishing it needs the author's agreement.
- Letters are drafts until an editor sends them in-app. Only staff see rounds;
  the submitter and linked co-authors see only sent letters, including subject,
  body and the optional longer mathematical note. No automatic send or email.
- A referee request goes to every configured Oracle pool; the first completed
  answer is the report.
- The NyxID Oracle task and submission identity are durable; the worker releases
  its lease between polls without spending retries. Transport completion is not
  mathematical verification. Model output is sanitised in the binary.
- Oracle CLI and Codex CLI backends are local-only and refused on non-loopback
  binds. Their children inherit only `PATH` and operator `HOME`, never deployment
  secrets. Codex operates on a fresh source copy with a separate scratch area,
  sandbox network disabled and a deadline; source edits are forbidden.
