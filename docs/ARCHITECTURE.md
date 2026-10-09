# Architecture

wishpool is a venue for mathematicians' own papers. An author uploads a LaTeX
source; the venue reads its statements, analyses which of them carry new
mathematical content, applies a published threshold, gives accepted papers a
public record, and, with the author's consent, formalizes valuable statements
in Lean. Volunteers may do part of the analysis with their own model tokens.

## 1. Crates

| Crate | Layer | Owns |
|---|---|---|
| `layer2-core` | 2 | domain (people, papers and versions, statements, stage reports, judgements, records, formalization plans, conjecture follow-ups, tasks, contributions, donations), the threshold `Policy`, the per-paper analysis, ports, services, authorization; in-memory ports |
| `layer1-public` | 1 | `/api/v1` REST projection, including multipart upload and file download |
| `layer3-latex` | 3 | unpack `.tex`/`.zip`/`.tar.gz` with limits; find the main file, inline `\input`/`\include`, read title, authors, abstract and statement environments; compile with TeX Live |
| `layer3-review` | 3 | OpenAI-compatible models through the NyxID LLM gateway (escape proposals, statement judgements, relating search candidates; metered usage); OpenAlex search |
| `wishpool` | binary | composition, MongoDB stores and GridFS files, NyxID sign-in and delegated tokens, the paper worker (compile, S2 leads, S3 proposals), the hosted donation worker, lease reconciliation |
| `sdk/contribute` | client | `wishpool-contribute`: CLI and MCP server for contributors' own agents |

Layer 2 depends on no workspace crate and no I/O crate; Layer 1 only on
Layer 2; Layer 3 crates on neither. CI enforces the rule. Layer 2 sees a
LaTeX reader only through the `PaperReader` port and stored files only
through `BlobStore`.

## 2. A paper

```
upload ──► draft ──confirm──► in_review ──decide──► accepted ──► formalization, conjectures
  │          ▲                    │                    (record WP-YYYY-NNNN, public page)
  │          └──── new version ───┴──────────────► not_accepted (private report)
  └─ S0: compile job (TeX Live) files the source report
```

- **Upload** (`submit_paper`): the source is stored in GridFS, read by the
  `PaperReader` (`layer3-latex`), and the statements are kept as `extracted`.
  A compile job is queued. At most three papers per author are in draft or
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
  old set.
- **S2 literature**: the worker searches OpenAlex for each main result and
  asks the model to relate the returned works (it may name only those
  works). The report is a proposal (`needs_human`); an editor files the
  binding report with prior works marked `same`, `implies` or `related`.
- **S3 escape**: judgements of each proved statement come from the worker
  (one pass over the source), contributors (one statement each, with the
  statements it depends on) and editors. They combine by an explicit rule
  (`judgement.rs`): an editor's judgement confirms; two machine judgements
  from different model families and accounts that agree corroborate; any
  disagreement without an editor is a dispute. An editor adopts the settled
  judgements as the S3 report.
- **Decision**: `Policy::decide` is a pure function of the reports. The
  author sees the same preview. Accepted papers get a record; the public page
  is assembled from the submission at read time, so formalizations added
  later appear on it.

## 3. The analysis

`PaperAnalysis::compute` gives, for each statement: the prior works the
literature report links to it and whether one states or implies it; the
editor's escape assessment; the combined judgement and its standing; and its
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
outcome and evidence. Follow-ups are public together with the analysis.

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
`sessions`, `login_attempts`, `review_jobs`, `referee_files`, and the GridFS bucket `papers`
(sources and PDFs; a source may exceed the 16 MB document limit). Every
aggregate replace is fenced on its revision. Jobs (`compile`,
`stage:literature`, `stage:escape`, `referee`) are upserted per paper and kind, leased
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
rounds and editor-sent letters. Each round binds the paper version and confirmed
statement revision. Layer 2 enforces input fencing, immutable settled steps,
referee → advice → letter ordering, and the positive recommendation gate. A
restart creates a new round and requeues work only after the current round has
settled. It preserves historical reports and letters. An admitted round may finish after an
editorial decision; new rounds are started only while the paper is in review.
If inputs change during a call, Layer 2 refuses the stale update and the worker
resumes the job against the current confirmed version.

Layer 3 owns neutral `Oracle` and `Advisor` contracts, permissive output types,
and shared mathematical instructions. The binary maps them onto domain types;
Layer 3 has no Layer 2 dependency. `OracleHttp` submits the whole compiled PDF
and confirmed statements to the existing NyxID Oracle relay and polls its task
id. `OracleCli` consumes the same server JSON through `nyxid oracle ask` and
`result`. HTTP and CLI refuse prompts over 500,000 characters or PDFs whose
base64 exceeds 12,000,000 bytes before sending. CLI commands have a 120-second
deadline and a cleared environment retaining only `PATH` and operator `HOME`.

`WISHPOOL_ORACLE_POOL` lists one or more pools (default
`chrono-chatgpt-pro-500-pool,company-chatgpt-pro`). The request goes to every
pool with the client reference suffixed by the pool name; the stored task
handle joins the task ids with commas. A poll reports the first completed
answer and cancels the other tasks (CLI only; the HTTP adapter leaves them to
finish), otherwise running, otherwise the best queue position; the step fails
only when every task failed. No model hint is sent: each pool applies its own
default label under `require_model_match`. `WISHPOOL_ORACLE_MODEL` is the label
recorded on the step and on referee judgements.

The worker persists client reference and prompt/PDF digest before submission,
then task id, queue position, engine/model and step attempts on success. The
identity `wishpool:<submission>:r<round>:v<version>:c<claims_revision>` recovers
uncertain submissions through Oracle's pool/submitter/client-ref deduplication.
Polling never resubmits a running task. Its original recorded model label is
retained for machine judgements even if worker configuration changes; changed
inputs/model after uncertain admission require a new round. Queued/running tasks and transient
transport or capacity failures release the job lease using `JobLease::defer`:
Mongo returns the fenced job to `queued` at `now + delay`, removes lease fields
and refunds that claim's attempt without recording an error. Memory mode holds
a delayed queue. This permits hours of Pro reasoning while the worker processes
other papers. Queue positions are persisted when they change; terminal reports
and raw answers remain downstream after Oracle content expires.

A completed positive report (`accept` or `minor_revision`) runs the advisor;
other recommendations, absent recommendations and failed referees skip advice.
A completed referee report can generate a letter after advice settles, including
negative recommendations and failed advice. A failed referee skips the letter.
Missing advisor configuration fails the advisor steps with a stated reason.
Referee failures preserve provider detail and retryability, and settled failures
need an editor restart. Malformed completed answers retain raw text and a parse
limit with no positive recommendation.

`CodexCli` receives a fresh workspace with a copy of the bounded, unpacked
source in `source/`, a named main file and report in `TASK.md`, and writable
`scratch/` for computations. Source copies are read-only and the prompt forbids
source edits. It runs ephemeral `codex exec` with `workspace-write`, explicitly
disabled sandbox network access, a cleared environment (`PATH`, `HOME` only),
null stdin, captured output, kill-on-drop and the configured deadline (default
1,200 seconds). It reads the final JSON from `answer.md`. Each advisor call
releases the job afterward so the letter gets a fresh 30-minute lease; operators
should keep the advisor timeout below that lease. Temporary workspaces are
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
cross-family `combine` rule. Referee recommendations, concerns and computation
claims never substitute for human S2/S3 adoption or `Policy::decide`. Acceptance
and author consent remain prerequisites to actual formalization.

After the advice, with `WISHPOOL_LEAN_WORKSPACE` naming a Lean project whose
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
judge. The probe is staff-only and is never a recorded formalization; the
letter may offer a compiled file to the author and publishing it needs their
agreement. The formal step as a whole is bounded by 25 minutes inside the
30-minute job lease, leaving five minutes to persist the result and defer the
job. Environment discovery commands each have a 30-second deadline. The Codex
deadline (`WISHPOOL_FORMAL_TIMEOUT_SECS`, default and maximum 1200 seconds)
leaves room for at most two Lean checks, each limited to 120 seconds.

The advisor's letter is an English draft with a short substantive body and an
optional longer mathematical note. Only editors send or edit letters, through
in-app delivery, and authors/co-authors see only sent messages. Staff see the
round history; strangers get `not_found`. No email is sent. CLI backends use
operator credentials and are refused on non-loopback binds. Referee rounds are
disabled unless an Oracle backend is configured, with one startup log message.
