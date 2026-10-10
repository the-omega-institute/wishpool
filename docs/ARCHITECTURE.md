# Architecture

wishpool accepts mathematicians' own papers, short notes and conjectures through
one endpoint and one referee → audit → decision → letter → Lean pipeline.
Short notes differ only in labels. Every kind shares the record sequence.
Public mathematical details are selected by a default-checked upload checkbox;
private review material never enters the public projection.

## 1. Crates

| Crate | Layer | Owns |
|---|---|---|
| `layer2-core` | 2 | domain (people, papers and versions, statements, stage reports, judgements, records, formalization plans, conjecture follow-ups, tasks, contributions, donations), the threshold `Policy`, the per-paper analysis, ports, services, authorization; in-memory ports |
| `layer1-public` | 1 | `/api/v1` REST projection, including multipart upload and file download |
| `layer3-latex` | 3 | unpack `.tex`/`.zip`/`.tar.gz` with limits; find the main file, inline `\input`/`\include`, read title, authors, abstract and statement environments; compile with TeX Live |
| `layer3-review` | 3 | OpenAI-compatible models through the NyxID LLM gateway (escape proposals, statement judgements, relating search candidates; metered usage); OpenAlex search |
| `wishpool-verifier` | 3 / binary | stateless Lean module checker, source scan, pinned workspace copy, bounded isolated processes, JSON receipt; no database or credentials |
| `wishpool` | binary | composition, MongoDB stores and GridFS files, NyxID sign-in and delegated tokens, the paper worker (compile, S2 leads, S3 proposals), the hosted donation worker, lease reconciliation |
| `sdk/contribute` | client | `wishpool-contribute`: CLI and MCP server for contributors' own agents |

Layer 2 depends on no workspace crate and no I/O crate; Layer 1 only on
Layer 2; Layer 3 crates on neither. CI enforces the rule. Layer 2 sees a
LaTeX reader only through the `PaperReader` port and stored files only
through `BlobStore`.

## 2. A submission

```
upload ──► draft ──confirm──► in_review ──decide──► accepted ──► formalization, conjectures
  │          ▲                    │                    (record WP-YYYY-NNNN, public page)
  │          └──── new version ───┴──────────────► not_accepted (private report)
  └─ S0: compile job (TeX Live) files the source report
```

- **Upload** (`submit_paper`): the source is stored in GridFS, read by the
  `PaperReader` (`layer3-latex`), and the statements are kept as `extracted`.
  Typed conjectures first generate a minimal `.tex` containing one main
  conjecture. The reader and GridFS/PDF path are identical to uploaded source.
  `kind` defaults to paper for old documents. `make_public_after_acceptance`
  maps to public/private and defaults true. A compile job is queued. At most three papers per author are in draft or
  review at once (`Policy::max_active_per_author`).
  The author may supply the paper's existing DOI, whether for a preprint or
  journal publication. Layer 2 strips DOI resolver and `doi:` prefixes, trims
  surrounding whitespace, and validates the bare identifier before storing it.
- **S0 source**: the worker compiles the current version with `pdflatex`
  (or the engine named in arXiv's `00README.json`), running BibTeX when the
  source cites a database and ships no `.bbl`, and rerunning while the log
  asks for it. The PDF is stored; the report lists the checks.
- **S1 statements**: the author confirms each extracted statement (kind,
  main or supporting, dependencies, an optional named open problem it
  settles) or excludes it. The confirmed set has a `claims_revision`; a
  changed set voids S2/S3 reports, judgements and contributor tasks of the
  old set. Paper/note confirmation requires a proved main result; standalone
  conjectures require a main open claim.
- **S2 literature / S3 escape**: GPT Pro referees, then Codex audits the report
  against the source offline. Layer 2 appends machine reports derived only from
  the audit: named prior works in S2; main-result shape, witnesses, correctness
  and rationale in S3. Gap/error/not-checked main results have no escape witness
  and cannot ground an open-problem settlement. Older report and contributor
  workflows remain available for optional editorial intervention. Conjecture
  prompts instead request per-claim well-posedness, open/known/unclear status,
  reported work names, content/bind-only escape and sharpening suggestions.
  The audit checks definitions, quantifiers, small cases and offline matches;
  unverifiable external knowledge becomes unclear.
- **Decision**: `Policy::decide` is the only authority. The default human stages
  are `{Claims}`; deployment policy may require humans again. The audited
  recommendation remains feedback even when it differs from the publication
  decision. The conjecture rule requires at least one main open claim audited
  well-posed, open and content; existing human and endorsement gates apply.
  Layer 2 applies the decision and public visibility before advice or letters, recovers
  partially written acceptance using the existing unique record per submission,
  and does not duplicate reports on replay. New accepted records pin immutable
  publication inputs, so revision uploads cannot change the existing public page.
  Revision uploads retain the publication's later verified proofs and conjecture
  follow-ups separately and clear the current version's formalization state;
  reused statement IDs never inherit a proof for the old inputs.
  For legacy records, the first revision preserves the publication inputs on the
  submission without modifying the record.

## 3. Public and private projections

`GET /papers/{record}` uses an allowlist independent of `PaperAnalysis`:
statements and dependencies, witness lemmas only for audited-correct main proved
content results, verified formal artifacts and author-confirmed conjecture Lean
statements. It carries no correctness labels, comments, rationale, audit summary,
referee report, letter, advice or follow-up text. The summary includes kind;
private and legacy undecided records serialize only record, title, authors and
kind, and their PDF is private. `/conjectures` filters accepted public conjecture
records, newest first with paging, one-line statements and target status.
The author can change visibility through the existing route.

`PaperAnalysis::compute` gives, for each statement: the prior works the
literature report links to it and whether one states or implies it; the
filed escape assessment; the combined judgement and its standing; and its
formalization state. Totals: main results, main results with content, main
results already known, open statements, verified formalizations.

Escape rates (`EscapeRateReading`) are exact ratios on an explicitly built
finite arena with a pinned artifact; the venue displays none unless an
editor files one.

## 4. Formalization and conjectures

After acceptance an editor proposes a statement with a reason; the author
approves or declines; an approved item may be worked on by the editors or,
when the author opted in, by contributors through a `formalize` task whose
pull request must target the paper's formalization repository. An editor
records the verified proof: repository, full commit, declarations, axioms
(only `propext`, `Classical.choice`, `Quot.sound`). The contribution named
there is credited.

Conjectures and questions the paper poses start in `screening` on
acceptance. An editor moves each to `taken_up` (opening a `probe` task when
the author opted in), `not_pursued` with a reason, or `settled` with an
outcome and evidence. These follow-ups remain private review material.
Solving uses the independent verifier, rather than editorial follow-up states.

Accepted standalone conjectures and audited paper problems queue durable
`lean_statement` work after the letter. Codex writes `Target.lean` with
`import Mathlib`, definitions, and `def wishpool_target_prop : Prop := ...`,
plus a plain-language reading. The binary independently elaborates the file
with no proof holes and rejects executable syntax and nonstandard axioms.
Legacy final-theorem targets convert mechanically when possible; every changed
file returns to author confirmation. Others regenerate with a correction.

Layer 2 stores source, SHA-256 of exact UTF-8 bytes, toolchain, reading and
version/claims revision. The submitting author must use cookie authentication to
confirm the exact current digest or reject with a nonempty correction. Rejection
queues regeneration and supplies the prior source and correction; a new version
clears targets and confirmation, including the pinned public page. Confirmed
statements are public text, never proof-verification badges.

## 5. Contributors

| Task | Opened | Counts when |
|---|---|---|
| `judge_escape` | per proved statement, when the author opts in and the paper is in review | corroborated or confirmed |
| `literature_check` | per main result, same conditions | an editor accepts it |
| `formalize` | when the author approves a formalization item | an editor records the verified proof naming it |
| `probe` | when an editor takes up a conjecture | an editor accepts it |

Leases are fenced by revision and expire (2 h, 4 h, 72 h, 24 h); a contributor
holds at most five and never contributes twice to one task. Authors cannot
take tasks on their own paper. Donated quota: the donor authorizes the venue
in NyxID with incremental consent; the refresh token is sealed with
AES-256-GCM; the hosted worker spends it on judgement and literature tasks
through the NyxID gateway and charges the reported usage to the donor's
monthly cap.

## 6. Storage and jobs

MongoDB collections: `people`, `submissions`, `endorsements`, `records`,
`counters`, `tasks`, `contributions`, `judgements`, `donation_grants`,
`sessions`, `login_attempts`, `review_jobs`, `referee_files`, `solve_files`,
`agent_entrants`, and the GridFS bucket `papers`
(sources and PDFs; a source may exceed the 16 MB document limit). Every
aggregate replace is fenced on its revision. Jobs (`compile`,
`stage:literature`, `stage:escape`, `referee`, `lean_statement`, `open_problems`, `verify_attempt`) are upserted per paper and kind, leased
for thirty minutes, retried with backoff and parked after three attempts.

## 7. Untrusted input

- Archives: at most 2,000 files and 64 MB unpacked; paths are normalised and
  may not leave the work directory.
- TeX: no shell escape; `openin_any=p` and `openout_any=p`; the child's
  environment is cleared (only `PATH`, `HOME` set to the work directory, the
  TeX cache, and an optional extra texmf tree); one deadline across passes.
  Only the failing excerpt of the log is shown to the author.
- Model output: sanitised in `review/mapping.rs`. A content judgement without
  a witness is dropped, not repaired; literature leads name only works
  OpenAlex returned, as DOIs or OpenAlex pages.

## 8. Deployment

One image (`infra/api/Dockerfile`, Debian trixie with TeX Live) serves two
roles: `api` (stateless replicas; parsing uploads happens here) and `worker`
(one replica; compiling, model calls, hosted donations, lease
reconciliation; it owns the TeX cache volume). NyxID provides sign-in, the
LLM gateway and delegated access; OpenAlex provides literature candidates.


## 9. Referee rounds

S1 confirmation also admits a `referee` job. `RefereeFile` is a separate
revision-fenced aggregate in `referee_files` (unique `id`), with chronological
rounds and delivered letters. Each round binds the paper version and confirmed
statement revision. Layer 2 enforces input fencing, immutable settled steps,
referee → audit → decision → advice → delivered letter → accepted-only formal
ordering. The model recommendation never gates publication or advice. A
restart creates a new round and requeues work only after the current round has
settled. It preserves historical reports and letters. An admitted round may finish after an
editorial decision; new rounds are started only while the paper is in review.
If inputs change during a call, Layer 2 refuses the stale update and the worker
resumes the job against the current confirmed version.

Layer 3 owns neutral `Oracle` and `Advisor` contracts, permissive output types,
and shared mathematical instructions. The binary maps them onto domain types;
Layer 3 has no Layer 2 dependency. `OracleHttp` submits the whole compiled PDF
and confirmed statements to the standalone Oracle broker through the NyxID
proxy (default base `https://nyx-api.chrono-ai.fun/api/v1/proxy/s/oracle`) and
polls its task id. `OracleCli` consumes the same broker JSON through
`nyxid proxy request oracle api/v1/oracle/...`: POST `pools/<pool>/tasks`,
GET `tasks/<id>`, and POST `tasks/<id>/cancel`. Request bodies, including the
PDF base64, use `--data @<file>` in a fresh temporary directory with mode-600
files removed after each request. HTTP sends `User-Agent: wishpool/<version>`.
HTTP and CLI refuse prompts over 500,000 characters or PDFs whose
base64 exceeds 12,000,000 bytes before sending. CLI commands have a 120-second
deadline and a cleared environment retaining only `PATH` and operator `HOME`.

`WISHPOOL_ORACLE_POOL` lists one or more pools (default
`chrono-chatgpt-pro-pool`). The request goes to every pool with the client
reference suffixed by the pool name; the stored task
handle joins the task ids with commas. A poll reports the first completed
answer and cancels the other tasks through either backend (404/409 are harmless),
otherwise running, otherwise the best queue position; the step fails
only when every task failed. No model hint is sent: each pool applies its own
default label under `require_model_match`. `WISHPOOL_ORACLE_MODEL` is the fallback
label recorded on the step and on referee judgements. A completed task's
observed model switcher and effort replace it when available (for example
`gpt_6 · pro`).

The worker persists client reference and prompt/PDF digest before submission,
then task id, queue position, engine/model and step attempts on success. The
identity `wishpool:<submission>:r<round>:v<version>:c<claims_revision>` recovers
uncertain submissions through Oracle's pool/submitter/client-ref deduplication.
Polling never resubmits a running task. Its original recorded model label is
retained as the fallback even if worker configuration changes; changed
inputs/model after uncertain admission require a new round. Queued/running tasks and transient
transport or capacity failures release the job lease using `JobLease::defer`.
Mongo returns the fenced job to `queued` at `now + delay`, removes lease fields
and refunds that claim's attempt without recording an error. Memory mode holds
a delayed queue. This permits hours of Pro reasoning while the worker processes
other papers. Queue positions are persisted when they change; terminal reports
and raw answers remain downstream after Oracle content expires.

HTTP 429 and 5xx map to transport errors. The proxy CLI can exit 0 on an HTTP
failure: an `error` key in stdout JSON still fails the request, with the status
read from stderr's `HTTP <code>`. Other HTTP failures retain only a sanitised
short error code, never the raw body or message.

Every completed referee report is audited by Codex, including a negative or
unparsed recommendation (the raw answer is retained). Missing Codex configuration
settles the audit without a decision; a chat-only advisor cannot perform this
audit. Failed referees skip dependent steps. Audit transport failures and outer budget timeouts retry the same job using
the existing job attempt limit, leaving dependent steps pending. Other audit
failures preserve a stated reason; optional editors may restart a settled failed
review. Advice runs for
every successfully audited paper after its decision is applied; failed advice
still permits a letter from the audited feedback.

`CodexCli` receives a fresh workspace with a copy of the bounded, unpacked
source in `source/`, a named main file and report in `TASK.md`, and writable
`scratch/` for computations. Source copies are read-only and the prompt forbids
source edits. It runs ephemeral `codex exec` with `workspace-write`, explicitly
disabled sandbox network access, a cleared environment (`PATH`, `HOME` only),
null stdin, captured output, kill-on-drop and the configured deadline (default
1,200 seconds for advice and letters). It reads the final JSON from
`answer.md`. While a step runs, the worker renews its 30-minute job lease about
every five minutes. Advice and letter deadlines are at most 3,600 seconds; the
audit has its own default 3,600-second, 7,200-second maximum deadline and outer
budget.
Prompts are bounded at 500,000 characters and answers at 2,000,000 bytes. Temporary workspaces are
removed after results are recorded. The production chat advisor uses the
existing gateway endpoint/token with its own model label and source text; it
has no computation tools and must identify those limits. Both use the same
prompts and sanitisation. Checked improvements require reported evidence and
outcomes; proposals carry empty evidence. Formalization candidates include
Mathlib notions, missing pieces and a Lean sketch, never a verification receipt.

Referee readings are filed once on completion as machine judgements under the
separate referee account (default `wishpool:referee`, engine `nyxid-oracle`).
The completed report is persisted before the individual judgement writes;
these writes are not one transaction. A crash or store failure between them
can leave some readings unfiled. Settled reports are not replayed, following
the rule that judgements are filed only on the transition to `done`.
Configuration refuses using the same account for the referee and the existing
review engine. They can corroborate another account's Claude readings through the existing
cross-family `combine` rule. Referee recommendations remain feedback. Codex audits their readings and files
S2/S3 through Layer 2, which applies `Policy::decide` automatically. Audit replay
uses report provenance and the unique record per submission. Publication of an
actual formalization still needs the author's agreement.

After the automatically delivered letter, only for accepted papers/notes, with `WISHPOOL_LEAN_WORKSPACE` naming a Lean project whose
Mathlib is built (and the Codex advisor), the round runs a private
formalization probe on at most two proposed, proved statements that the advice
did not call hard, most tractable first. Codex works in a fresh directory with a
read-only source copy, writes `lean/<claim>.lean`, and compiles with a
`check.sh` that runs the workspace's `lean` with the workspace's `LEAN_PATH`.
The binary then checks every file itself: it refuses `sorry`, `admit`, `axiom`
and `opaque` declarations, `implemented_by`, `extern`, `unsafe`, `#exit` and
`debug.skipKernelTC`, compiles the file with `#print axioms` on the named
theorem, and records `compiled` only when Lean reports no error and the
theorem depends on nothing outside `propext`, `Classical.choice` and
`Quot.sound`. The prover's statement of faithfulness is recorded as its note;
whether the Lean statement is the paper's is for the editor and the author to
judge. The probe files are staff-only and never a recorded formalization. Authors
receive only claim, outcome and theorem name; publishing files needs their
agreement. No second letter is required. The formal step budget is the
configured `WISHPOOL_FORMAL_TIMEOUT_SECS` plus a margin for environment
discovery and Lean checks. The timeout defaults to 1,200 seconds and is capped
at 3,600 seconds. Environment discovery commands each have a 30-second
deadline; each Lean check is limited to 120 seconds.

The advisor writes an English letter after the applied decision, with a short
body and optional mathematical note. Layer 2 prepends the authoritative decision
and atomically stores the done letter and its sent message (auditor sender,
audited assessment, round, `edited=false`). Confirmed concerns become requested
changes; unavailable facts become "please check"; refuted concerns are omitted
as requests. Advice supplies improvement suggestions; Lean has not run yet.
Editors may still send additional letters. Delivery is in-app; no email transport
is used. Staff see full history; authors and linked co-authors get the report,
audit and projected probe outcomes, with no advice, drafts, provider metadata or
Lean files. Strangers get `not_found`. CLI backends use operator credentials and
are refused on non-loopback binds. Referee rounds are disabled unless an Oracle
backend is configured, with one startup log message.

## 10. Paper problems, attempts, and scoring

A `SolveFile` binds a source record, claim, paper version, and claims revision.
For accepted public papers/notes, it holds the candidate's separately sanitized
referee report, Oracle task handle, and Codex audit. The task reference includes
those inputs and a review round, so uncertain transport admission reuses the
same identity. Each claim is independently reviewed against the complete paper
source. `Policy::admits_open_problem` is the shared well-posed/open/content
predicate. A request marker saved with new acceptance (or an explicit admin
trigger or an author's explicit switch to public visibility) lets reconciliation repair missed candidate/target enqueues without
backfilling unrequested legacy papers. Each admitted paper problem uses its own
report for target preparation, including old papers without a primary letter.
Reports stay private. Only admitted candidates appear in public
conjecture projections. Failed candidates can be retriggered by an admin.
Version uploads invalidate targets and pending results; a new accepted version
gets new candidate files. Ordinary paper pages retain immutable publication
inputs, while conjecture pages bind the current accepted source version.

Attempts store solution blobs in GridFS and metadata/receipts on the
revision-fenced solve aggregate. Submission performs agent-ownership checks,
validates size/note limits, enforces the preceding-24-hour entrant quota, and
reuses an attempt for the same entrant/target/solution digest. Each attempt has
its own `verify_attempt` job. Queue reconciliation repairs an enqueue-after-save
crash window. The worker renews and rechecks its lease before recording results,
and rechecks the target after verification. Client/model success claims never
cross the verifier port. Rejected attempts stay private; verified ones expose
only public identity, source, and receipt, without the private submission note.

Obsolete pending attempts are terminal private `superseded` records, with a reason
and no fabricated verifier receipt. Reconciliation repairs missing job enqueues
only for current inputs. Public entrant names resolve the latest agent/person
identity; renaming or retiring an agent preserves credited ids.

The Layer-3 adapter uses a bounded subprocess locally or the isolated verifier
HTTP service. The verifier clears its compiler environment to PATH and a fresh
HOME, copies the pinned prepared workspace, verifies toolchain/Mathlib identity,
and builds Target, Solution, then Check as separate modules. Generated `--setup`
files resolve only pinned artifacts without inherited LEAN_PATH. Check uses the
imported root target constant, typed `#check`, and `#print axioms`; only the
three standard axioms pass. All subprocess output is capped, the whole check
has a deadline, and Lean concurrency is at most two. The service accepts no
credentials and serializes checks. Its Deployment has no secrets or service
account token; NetworkPolicy denies every egress destination.

The earliest receipt timestamp (attempt id on an exact tie) selects one winner
per confirmed target. Public status, in-app author notifications, profile solve
lists, and leaderboard credit derive from that same winner. Later successes are
“also verified”. One board ranks people and owned agents by proved + disproved,
then earlier last solve and entrant id. Month means the current UTC calendar
month. Visibility and version fences apply to all public solve projections.
Author-confirmed cross-work S1 dependencies retain source version and claim
revision, and a distinct-public-work downstream count; weighting is deferred.
