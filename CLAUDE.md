# wishpool

This file is the contract for anyone — human or agent — working in this
repository. Rules here are binding.

## 1. What wishpool is

A venue where mathematicians submit their own papers, short notes and
conjectures through one entry and one review pipeline. The venue reads their
statements, audits the referee's mathematical reading, and gives accepted work
one shared `WP-<year>-NNNN` record sequence. Short notes follow exactly the paper
pipeline; only their labels differ.

- **Input**: papers and notes upload `.tex`, `.zip` or `.tar.gz` sources.
  Conjectures use the same endpoint with either LaTeX source or a typed title,
  statement (at most 20,000 characters), optional background (50,000) and origin.
  Typed input becomes a minimal stored `.tex`, using the same reader, blob store,
  PDF build and author confirmation. Its single conjecture is a main claim.
- **Statements**: the author confirms kind, main or supporting role and
  dependencies. Papers/notes require a proved main result; conjectures require a
  main conjecture or question. Stored documents without `kind` load as `paper`.
- **Analysis**: for papers/notes, whether prior work states or implies a result,
  and whether its proof carries a new intermediate proposition (an escape
  witness) or follows by binding alone. Conjectures are checked for well-posedness,
  open/known status, and whether a proof would carry new mathematical content.
  Named works are reported names, never verified bibliographic claims.
- **Public layers**: the upload checkbox “Make public after acceptance” defaults
  to checked for all kinds. Publication happens at the decision, before advice,
  letter or Lean. The author may later change visibility through the existing
  route. Public details contain title, authors, abstract, DOI and record id;
  statements with dependencies; witness lemmas of audited-correct main content
  results; and verified Lean artifacts. Lean ✓ means a verifier-passed proof.
  Correctness labels, comments, rationale, audit summaries, referee reports,
  letters and advice stay private to authors/staff. Private or legacy undecided
  records expose title, authors, kind and record id only.
- **After acceptance**: papers/notes receive the existing private post-letter
  Lean probe; publishing proofs still requires author approval. Accepted
  conjectures receive a Lean target and plain-language reading after the letter.
  The binary elaborates `Target.lean`: `import Mathlib`, confirmed definitions,
  and `def wishpool_target_prop : Prop := <statement>`. No proof hole is allowed.
  It stores exact text, SHA-256 digest and pinned Lean/Mathlib revision. Only the
  submitting author in a cookie session confirms the digest or rejects it with
  a correction. A new version voids confirmation. Legacy theorem targets are
  converted mechanically when possible, independently elaborated, and confirmed
  again; other legacy targets are regenerated.
- **Open problems from papers**: every author-confirmed conjecture/question in
  an accepted public paper/note that the paper does not settle is a candidate.
  A durable, version/claims-revision-fenced job runs a separate GPT Pro conjecture
  referee and Codex source audit for each claim, using the same sanitizers and
  Policy well-posed/open/content rule as submitted conjectures. Only passing
  candidates enter `/conjectures`, with their source record and claim id. The
  paper's submitting author confirms their exact Lean target. Private paper
  problems never enter the list. New versions require a fresh check. An admin
  may trigger checks for already-accepted papers; there is no automatic backfill.
- **Contributors**: when the author opts in, volunteers do the legwork
  (judgements, literature checks, probes, formalizations) with their own
  model tokens or donated NyxID quota, credited only when verified.

Review stages:

| Code | Stage | Filed by |
|---|---|---|
| S0 | Source: the source is read, the PDF compiles, AI use is disclosed | worker (TeX Live) |
| S1 | Statements and their dependency graph | **the author** |
| S2 | Literature: no main result is already stated or directly implied | **Codex auditor** (checks the GPT Pro report against the source) |
| S3 | Escape: bind-only vs content, escape witnesses | **Codex auditor** (source-based correctness, shape and witnesses) |

Threshold (`api/crates/layer2-core/src/policy.rs`, the only authority):
accept on **escape_witness** (a main result is judged content and is not
known) or **open_problem_settlement** (a main result settles a named, sourced
open problem the literature had not settled). Conjectures are accepted for
**open_conjecture** iff at least one main open claim is audited well-posed,
`open`, and `content` (a proof would carry new mathematical content). Known true,
known false, special cases of known results, unclear status and bind-only readings
do not meet that rule. Existing human-stage and endorsement gates apply to every
kind. Every non-acceptance carries an author-facing reason.

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
  layer3-review/   Layer 3: models via NyxID: Oracle broker and CMA.
  verifier/        stateless, credential-free Lean checker and binary.
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
- Accepted mathematical details are public when the upload checkbox selected
  `public`, and may be made private later. Reviews and letters remain private
  under either setting. Formalization of a proved statement starts only after
  the author approves it. The private post-letter Lean probe exposes outcomes, never proof
  files, to the authors; publishing files still requires their agreement.
- Submitter and linked co-authors may read their private audit, the GPT Pro
  report and projected Lean outcomes through the referee view. Advice and probe
  files remain staff-only; public acceptance does not expose that private view.
- Only the author confirms statements (S1). A new version returns the paper
  to draft; a changed statement set voids S2/S3 reports, judgements and tasks
  of the old set by revision, never by deletion.

## 5. Judgements and credit

- Escape judgements retain provenance; never present them as Lean-checked.
  GPT Pro referees and Codex audits its report against the source. Layer 2
  files machine S2/S3 from that audit and automatically applies `Policy::decide`.
  Only a proved main result audited `correct` can carry escape witnesses or
  ground a named open-problem settlement. Standalone conjecture admission uses
  the audited well-posed/open/content reading, with no proof-correctness claim. Recommendations are feedback, never decisions.
  The default human requirement is `{Claims}`; deployments may require human
  S2/S3 reports again. Editors are optional. Independent contributor judgements
  still corroborate by the existing different-family/account rule; editors may
  confirm them or resolve disputes.
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
  named prior works include the DOI, arXiv id or URL the model states it opened.
  Those sources are reported, never venue-verified. External facts the model
  could not open remain `not_checkable`. Never send secrets, credentials, token
  files or deployment configuration to CMA; only bounded paper/review data.

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
  statement revision. Layer 2 owns ordering, input fencing, immutable settled
  steps, report filing, decision application, letter validation and view rules.
- The pipeline is GPT Pro referee → Codex audit → automatic escape-analysis
  decision → advice → automatically delivered letter → accepted-only Lean probe.
  Publication is immediate when the selected visibility is public. Paper/note
  probes and conjecture statement elaboration follow the delivered letter.
  No step requires a human editor. Editors/admins may intervene, restart settled
  reviews while the paper is in review, or send additional letters.
- For conjectures, the referee supplies well-posedness reasons, open/known/unclear
  status, reported work names, content/bind-only escape reasons and sharpening
  suggestions. CMA Codex checks missing symbols/quantifiers and small cases, and
  may search literature and open sources. Known-result matches require a
  matching argument and the reported opened source, or reproducible local
  mathematical evidence. Unopened external knowledge becomes unclear.
- The audit checks source, report JSON and full text, computing where useful.
  It covers every confirmed statement with correctness, a one-sentence comment,
  proof shape/witnesses where applicable, and agreement with the referee.
  External facts the agent could not open are `not_checkable`. Every named
  prior work includes its reported opened source; never invent citations.
- Layer 2 appends audit-derived S2 (known prior works) and S3 (main-result escape
  assessments) under the statement revision fence. Gap/error/not-checked main
  results have no escape witnesses and cannot ground acceptance. Publication is
  decided only by `Policy::decide`, never either model's recommendation.
  Report and decision replay is idempotent, including recovery after record
  insertion. Accepted records and their publication inputs remain immutable.
- Advice runs after the applied decision for every audited paper and feeds
  improvement suggestions to the letter. Formalization candidates are used only
  after acceptance. A sketch does not verify a theorem.
- A completed letter step atomically delivers the letter in-app, with its round,
  audited assessment, the auditor as sender, and `edited=false`. It states the
  applied decision and record/reasons first, then audited feedback, confirmed
  changes, "please check" for unavailable facts, and improvement suggestions.
  It contains no Lean results. Manual additional letters remain available.
- Lean runs only after a delivered letter and acceptance, using advice's proved
  candidates. The binary compiles each file and checks axioms. Authors see
  statement, outcome and theorem name afterwards; no second letter is needed.
  A compiled probe is not a recorded formalization; publishing files needs the
  author's agreement.
- Staff receive full review history. Submitter and linked co-authors receive
  reports, audits, sent letters and projected Lean outcomes through the existing
  referee GET; advice, drafts, provider metadata and Lean files remain private.
  Anyone else receives `not_found`, including readers of an accepted paper.
- Referee readings remain machine judgements under `wishpool:referee` by default
  (engine `nyxid-oracle`). Audit reports and automatic letters use the separate
  `wishpool:auditor` account by default (engine `nyxid-cma`; `codex-cli` only for explicit development fallback). All model output is
  untrusted and sanitised in the binary; never invent witnesses or citations.
- A referee request goes to every configured Oracle pool; the first completed
  answer is the report. The default pool is `chrono-chatgpt-pro-pool` on the
  standalone Oracle broker, reached through NyxID's proxy service `oracle`
  (`nyxid proxy request oracle api/v1/oracle/...` for the local CLI).
- The NyxID Oracle task and submission identity are durable; the worker releases
  its lease between polls without spending retries and renews a running job's
  30-minute lease about every five minutes. A lost lease fences the pending
  result. Transport completion is not mathematical verification. Model output
  is sanitised in the binary.
- All Codex agent steps use CMA through NyxID: audit, advice, letter,
  accepted-paper Lean probe, conjecture target generation and paper-problem
  audits. CMA network access is on. Paper TeX and needed inputs travel inline,
  split into turns above 200 KiB. Final messages are JSON; Lean entries carry
  every file's complete text. Answers are capped at 2,000,000 bytes.
- CMA agent/response ids, chunk progress, deadlines and terminal answers are
  durable private steps in the referee aggregate. Stable keys bind submission,
  version, statement revision, round, step and attempt. Worker restarts resume
  polling with those ids; uncertain mutations reuse the original key and body.
  Agent steps have at most 3,600 seconds. The lease heartbeat stays active.
- CMA output is untrusted: the binary writes returned Lean text into fresh
  scratch and checks it locally; the credential-free solution verifier is
  unchanged. Always stop and delete the CMA Agent after the final answer or on
  cancellation/failure, best effort with logged failures. Credentials stay in
  the transport, never in a model prompt.
- `WISHPOOL_AGENT_BACKEND=cma|codex-local` selects the carrier; configuring a
  CMA workspace defaults to CMA. `codex-local` is an explicit development
  fallback, refused on non-loopback binds. Oracle CLI and CMA CLI are also
  loopback-only. CLI children inherit only operator `PATH` and `HOME`.
  The local fallback uses fresh source/scratch copies and disables its sandbox
  network. Production CMA HTTP uses a NyxID user access token from the existing
  sealed delegated-token refresh path or a mounted token file; agent keys are
  rejected. Every new configuration key must be declared in infra (§8).

## 12. Solving and the leaderboard

- Only a verifier-passed Lean proof or disproof of the author-confirmed target
  counts. The API uses the Layer-3 `Verifier` port and never accepts a client's
  or model's claim of success. Solutions import `Target` and declare exactly one
  `wishpool_solution : wishpool_target_prop` or
  `wishpool_disproof : ¬ wishpool_target_prop`; auxiliary lemmas and definitions
  may precede it.
- The verifier builds Target and Solution as separate modules in a fresh copy
  of the pinned Lean workspace, then independently checks the imported constant
  against `_root_.wishpool_target_prop` and prints its axioms. Only `propext`,
  `Classical.choice`, and `Quot.sound` are allowed. Reject axioms, constants,
  opaque/unsafe declarations, sorry/admit, implementation overrides, extern,
  macros/notation/syntax/elaborators/initializers, weakened options, executable
  commands, and imports other than Mathlib and Target.
- The verifier is stateless and credential-free: cleared environment (PATH and
  fresh HOME only during compilation), fresh work directory, bounded input and
  output, and a deadline (default 1200 seconds, maximum 3600). Local workers use
  a subprocess. Infrastructure uses a separate Deployment with no mounted
  secrets or service-account token and a NetworkPolicy denying all egress.
- Attempts are durable leased jobs, private to their owner/staff until verified,
  limited per entrant and conjecture, and idempotent by entrant, target digest,
  and solution digest. A revision-fenced write records the verifier receipt.
  The earliest verifier timestamp wins; an attempt id breaks an exact timestamp
  tie. Later checked attempts are public as “also verified” and earn no points.
  The author receives an in-app notification. Public solutions and profiles
  contain no private notes, referee reports, audit readings, or letters.
- One leaderboard contains people and pure agents. Every agent is owned by a
  signed-in person; AI assistance does not change a person's entrant kind.
  Score is proved plus disproved conjectures, with no votes or stars. Filters
  select all/people/agents and all time/current UTC month. Equal scores sort by
  earlier last solve, then entrant id. Public profiles link winning solutions
  and receipts. Store author-confirmed S1 dependency edges from other public
  accepted works and their distinct-work downstream count for later weighting;
  those counts do not affect today's score.
