# Wishpool HTTP API

The browser and every other client use the same public routes. The web app is
served from the same origin as the API; there is no private route.

- Base path: `/api/v1`. JSON request and response bodies, except the two
  upload routes (`multipart/form-data`) and file downloads.
- Authentication: the `wp_session` cookie (set by the NyxID sign-in flow) or
  `Authorization: Bearer <NyxID access token>` for machine clients.
- Cookie-authenticated `POST`/`PUT`/`PATCH`/`DELETE` requests must carry an
  `Origin` header equal to the deployment's public origin (browsers do this).
- Errors are `application/problem+json`:

```json
{ "type": "urn:wishpool:problem:conflict", "title": "conflict", "status": 409,
  "detail": "awaiting S2: S2 needs a human report", "code": "conflict" }
```

| `code` | status |
|---|---|
| `not_authenticated` | 401 |
| `forbidden` | 403 |
| `not_found` | 404 (also returned for papers the caller may not see) |
| `invalid` | 422 |
| `conflict`, `stale_revision` | 409 |
| `unavailable` | 503 |

- Listings return `{ "items": [...], "next_before": "<cursor>" | null }`, newest
  first. Pass `?before=<cursor>&limit=<1..100>` for the next page.
- Absent optional fields may be omitted or `null`.
- Timestamps are RFC 3339 UTC strings. Enums are `snake_case` strings; tagged
  unions carry their tag in the field named below (`state`, `outcome`,
  `decision`, `stage`, `kind`, `output`, `reason`).

## Sign-in (`/auth`, served by the binary)

| Method | Path | Behaviour |
|---|---|---|
| GET | `/auth/login?return_to=/path` | 302 to NyxID (authorization code + PKCE). `return_to` must be a same-origin path. |
| GET | `/auth/callback` | NyxID redirect target. Sets `wp_session`, 302 to `return_to`. |
| GET | `/auth/session` | `{ "authenticated": false, "donations_enabled": bool, "dev_sign_in": bool }` or `{ "authenticated": true, "person": Person, "donations_enabled": bool, "dev_sign_in": bool }`. `dev_sign_in` is true only in local development mode, where `/auth/login?as=<name>` signs in as `dev:<name>` without a provider. |
| POST | `/auth/logout` | 204; clears the session. |
| GET | `/auth/donate?cap=<tokens>&model=<name>` | Signed in, same-site navigation only. Incremental NyxID consent for delegated model access; 302 back to `/contribute`. |

## The life of a paper

1. **Upload** (`POST /submissions`): the author sends the LaTeX source (`.tex`,
   `.zip` or `.tar.gz`, at most 30 MB) and metadata. The server reads the title,
   authors, abstract and every theorem-like environment. The paper is a
   `draft`; its statements are in `extracted`. The PDF compiles in the
   background and S0 (source) is filed.
2. **Confirm** (`POST /submissions/{id}/claims`): the author confirms each
   extracted statement (kind, main result or supporting, dependencies) or
   excludes it. At least one proved statement must be a main result. The
   paper goes `in_review`; the confirmed statements are `claims`.
3. **Review**: a referee round on the confirmed version and statements, alongside S2 literature and S3 escape analysis. Machines draft (S2 leads
   from OpenAlex, S3 judgements); editors decide. Judgements of each
   statement are combined: an editor's judgement confirms; two machine
   judgements from different model families and accounts that agree
   corroborate. `POST /escape/adopt` files S3 from the settled judgements.
4. **Decision** (`POST /submissions/{id}/decision`): the threshold is applied.
   Accepted papers get a record `WP-<year>-NNNN` and a public page; others are
   `not_accepted` with reasons, kept private, and may upload a new version.
5. **After acceptance**: the author chooses whether readers see the analysis
   (`PUT /visibility`). Editors propose statements to formalize; the author
   approves or declines each; a verified Lean proof shows on the public page.
   The paper's conjectures are followed up (`screening`, `taken_up`,
   `not_pursued`, `settled`); their state is public with the analysis.

**Threshold** (`GET /policy`, `api/crates/layer2-core/src/policy.rs`): accept
when some main result carries an escape witness (judged `content`) and the
literature check found no work that states it (`same`) or directly implies it
(`implies`); or when a main result settles a named, sourced open problem that
the literature had not settled.

## Routes

| Method | Path | Who | Body → Response |
|---|---|---|---|
| GET | `/policy` | anyone | → `{ policy, stages: [{stage, code, title, description}] }` |
| GET | `/me` | signed in | → `Person` |
| GET | `/people` | admin | → `Listing<Person>` |
| PUT | `/people/{id}/roles` | admin | `{ roles: Role[] }` → `Person` |
| POST | `/submissions` | signed in | multipart: `metadata` (JSON `NewPaper`), `source` (file) → 201 `Submission` |
| GET | `/submissions?scope=mine\|queue` | signed in; `queue`: editors, reviewers, admins | → `Listing<Submission>` |
| GET | `/submissions/{id}` | authors, staff | → `Submission` |
| POST | `/submissions/{id}/claims` | submitting author | `{ claims: ClaimConfirmation[] }` → `Submission` |
| POST | `/submissions/{id}/versions` | submitting author (draft, in review, not accepted) | multipart: `source` (file), `note` (text) → `Submission` (back to `draft`) |
| POST | `/submissions/{id}/withdraw` | submitting author (draft, in review) | → `Submission` |
| PUT | `/submissions/{id}/contributors` | submitting author | `{ open: bool }` → `Submission` |
| PUT | `/submissions/{id}/visibility` | submitting author, accepted | `{ visibility: "public"\|"private" }` → `Submission` |
| GET | `/submissions/{id}/referee` | authors, staff | → `RefereeFile`; authors receive only sent letters (`rounds: []`) |
| POST | `/submissions/{id}/referee/restart` | editors, in review | → `RefereeFile`; 409 while the current round is unsettled |
| POST | `/submissions/{id}/referee/letters` | editors, any status except withdrawn | `{ subject, body, note? }` → 201 `FeedbackLetter` (in-app delivery) |
| GET | `/submissions/{id}/analysis` | authors, staff | → `PaperAnalysis` |
| GET | `/submissions/{id}/files/pdf?version=` | authors, staff; anyone once accepted | → `application/pdf` (inline) |
| GET | `/submissions/{id}/files/source?version=` | authors, staff | → the uploaded archive (attachment) |
| POST | `/submissions/{id}/stages/{stage}/reports` | editors (human), reviewer accounts (machine, with `filed_by`) | `{ report: ReportDraft, filed_by?: {engine, model?} }` → `Submission` |
| POST | `/submissions/{id}/claims/{claim}/judgements` | editors | `{ shape, witnesses, rationale }` → 201 `ClaimJudgement` |
| POST | `/submissions/{id}/escape/adopt` | editors | → `Submission` (S3 filed from settled judgements) |
| GET | `/submissions/{id}/decision` | authors, staff | → `Decision` (preview) |
| POST | `/submissions/{id}/decision` | editors | → `Submission` |
| GET, POST | `/submissions/{id}/endorsements` | GET authors, staff; POST endorsers | `NewEndorsement` → 201 `Endorsement` |
| PUT | `/submissions/{id}/formalization/repository` | editors, accepted | `{ repository: url }` → `Submission` |
| POST | `/submissions/{id}/formalization/items` | editors, accepted | `{ claim, reason }` → `Submission` |
| POST | `/submissions/{id}/formalization/items/{claim}/response` | submitting author | `{ approve: bool, reason?: string }` → `Submission` |
| POST | `/submissions/{id}/formalization/items/{claim}/start` | editors | → `Submission` |
| POST | `/submissions/{id}/formalization/items/{claim}/verification` | editors | `{ artifact: FormalArtifact, axioms: string[], contribution?: id }` → `Submission` |
| PUT | `/submissions/{id}/conjectures/{claim}` | editors, accepted | `{ state: ConjectureState }` → `Submission` |
| POST | `/submissions/{id}/tasks` | editors; the author opted in | → `{ created, existing }` |
| GET | `/papers` | anyone | → `Listing<PaperSummary>` |
| GET | `/papers/{record}` | anyone | → `PublicPaper` |
| GET | `/tasks?kind=&status=&submission=&holder=` | signed in | → `Listing<Task>` |
| GET | `/tasks/{id}` | staff; contributors while the author keeps the paper open | → `TaskContext` |
| POST | `/tasks/{id}/lease` | signed in, not an author | optional `{ mode: "own_agent"\|"hosted" }` → `Task` |
| DELETE | `/tasks/{id}/lease` | lease holder | → `Task` |
| POST | `/tasks/{id}/contributions` | lease holder | `NewContribution` → 201 `{ contribution, nature }` |
| GET | `/contributions?task=&contributor=&status=&kind=` | signed in | → `Listing<Contribution>` |
| POST | `/contributions/{id}/review` | editors | `{ accept: bool, note }` → `Contribution` (literature checks and probes) |
| GET | `/contributors?contributor=` | anyone | → `Credit[]` |
| GET, PATCH | `/donation` | signed in | PATCH `{ monthly_cap?, status?: "active"\|"paused"\|"revoked" }` → `DonationGrant \| null` |

`stage` in paths is `hygiene`, `literature` or `escape` (`claims` belongs to
the author's confirm route). Stage codes: S0 hygiene (source), S1 claims
(statements), S2 literature, S3 escape.

## Types

The optional paper `doi` is its existing persistent identifier, for example
`10.48550/arXiv.2609.33421` or a journal DOI. Uploads accept a bare DOI, a DOI
resolver URL, or a `doi:` prefix. Layer 2 trims surrounding whitespace and
strips the prefix; an empty value is absent. The bare DOI must start with
`10.`, have a slash with a non-empty suffix, contain no whitespace, and be at
most 256 characters. Invalid values return `invalid` with `"<value>" is not a DOI`.

```ts
type Role = "editor" | "endorser" | "reviewer" | "admin";
type Person = { id: string; display_name: string; email?: string; picture?: string;
  orcid?: string; affiliation?: string; roles: Role[]; created_at: string; last_seen_at: string };

type NewPaper = {
  ai_disclosure: { level: "none" | "assisted" | "substantial" | "primarily"; statement: string };
  authors?: Author[];          // overrides \author
  msc?: string[];              // e.g. "11B83"
  doi?: string | null;         // persistent identifier, when the paper already has one
  open_to_contributors?: boolean;
};
type Author = { name: string; person?: string; orcid?: string; affiliation?: string };

type ClaimKind = "theorem" | "proposition" | "lemma" | "corollary" | "claim" | "conjecture" | "question";
type Claim = { id: string /* C1, C2, … */; kind: ClaimKind; label: string; latex_label?: string;
  statement: string /* LaTeX */; role: "main" | "supporting"; has_proof: boolean; section?: string;
  depends_on: string[]; settles?: { name: string; source: Source } };
type ClaimConfirmation = { id: string; kind: ClaimKind; role: "main" | "supporting";
  depends_on?: string[]; settles?: { name: string; source: Source } | null; excluded?: boolean };
type Source = { kind: "doi" | "arxiv" | "hexagon" | "zenodo" | "oeis" | "url" | "personal"; locator: string; year?: number };

type Submission = {
  id: string; submitter: string; title: string; abstract_text: string; authors: Author[];
  ai_disclosure: NewPaper["ai_disclosure"]; msc: string[]; doi?: string;
  versions: PaperVersion[];   // the last is current
  extracted: Claim[];         // read from the current version, awaiting confirmation
  claims: Claim[];            // confirmed
  claims_revision: number;
  reports: StageReport[];     // append-only; the latest per stage for the current claims counts
  status: { state: "draft" } | { state: "in_review" } | { state: "accepted"; record: string }
        | { state: "not_accepted" } | { state: "withdrawn" };
  decision?: Decision;
  open_to_contributors: boolean;
  analysis_visibility: "undecided" | "public" | "private";
  formalization: { repository?: string; items: FormalizationItem[] };
  conjectures: ConjectureFollowUp[];
  created_at: string; updated_at: string; revision: number };
type PaperVersion = { number: number; archive: Blob; filename: string; main_file: string;
  pdf?: Blob; compile_error?: string; parse_warnings: string[];
  macros: Record<string, string>;   // preamble math macros, KaTeX syntax ("\\rep": "\\operatorname{rep}")
  note: string; uploaded_at: string };
type Blob = { id: string; bytes: number; sha256: string };

type StageReport = { stage: "hygiene" | "claims" | "literature" | "escape";
  outcome: { outcome: "pass" } | { outcome: "fail"; reason: RejectReason } | { outcome: "needs_human"; question: string };
  summary: string; payload: StagePayload; evidence: Evidence[];
  reviewer: { kind: "human"; person: string } | { kind: "machine"; account: string; engine: string; model?: string };
  claims_revision: number; filed_at: string };
type StagePayload =
  | { stage: "hygiene"; checks: { name: string; passed: boolean; detail: string }[] }
  | { stage: "claims"; claims: Claim[] }
  | { stage: "literature"; prior: PriorWork[]; searched: string[] }
  | { stage: "escape"; assessments: EscapeAssessment[] };
type ReportDraft = { outcome: StageReport["outcome"]; summary: string; payload: StagePayload; evidence?: Evidence[] };
type PriorWork = { claim: string; source: Source; relation: "same" | "implies" | "related"; note: string };
type EscapeAssessment = { claim: string; shape: "bind_only" | "content"; witnesses: string[]; rationale: string;
  escape_rate?: { arena: string; before: Ratio; after: Ratio; artifact: string } };
type RejectReason = { reason: "hygiene"; detail: string } | { reason: "known_result"; claim: string; prior: Source }
  | { reason: "bind_only" } | { reason: "no_main_result" } | { reason: "out_of_scope"; detail: string };
type Decision = { decision: "pending"; awaiting: StageReport["stage"]; detail: string }
  | { decision: "accept"; basis: "escape_witness" | "open_problem_settlement" }
  | { decision: "not_accepted"; reasons: RejectReason[] };

type ClaimJudgement = { id: string; submission: string; claim: string; claims_revision: number;
  shape: "bind_only" | "content"; witnesses: string[]; rationale: string;
  reviewer: StageReport["reviewer"]; task?: string; filed_at: string };
type PaperAnalysis = { submission: string; main_results: number; main_with_content: number;
  main_known: number; open_statements: number; formalized: number;
  claims: { claim: string; kind: ClaimKind; role: "main" | "supporting"; label: string;
    prior: PriorWork[]; known: boolean; assessment?: EscapeAssessment;
    judgement?: { shape?: "bind_only" | "content"; standing: "confirmed" | "corroborated" | "proposed" | "disputed";
                  witnesses: string[]; judgements: number };
    formalization?: ItemState }[] };

type FormalArtifact = { repository: string; commit: string /* 40 hex */; declarations: string[] };
type ItemState = { state: "proposed" } | { state: "approved" } | { state: "declined"; reason: string }
  | { state: "in_progress" } | { state: "verified"; artifact: FormalArtifact; axioms: string[]; contribution?: string };
type FormalizationItem = { claim: string; reason: string; state: ItemState; updated_at: string };
type ConjectureState = { state: "screening" } | { state: "taken_up" } | { state: "not_pursued"; reason: string }
  | { state: "settled"; outcome: "proved" | "disproved" | "partial"; summary: string; evidence: Evidence[] };
type ConjectureFollowUp = { claim: string; state: ConjectureState; updated_at: string };

type PaperSummary = { record: string; submission: string; title: string; authors: Author[];
  abstract_text: string; msc: string[]; doi?: string; basis: "escape_witness" | "open_problem_settlement";
  accepted_at: string; main_results: number; lean_verified: number };
type PublicPaper = { summary: PaperSummary; ai_disclosure: NewPaper["ai_disclosure"];
  versions: { number: number; uploaded_at: string; has_pdf: boolean; note: string }[];
  claims: { id: string; kind: ClaimKind; role: "main" | "supporting"; label: string; statement: string;
            section?: string; lean?: FormalArtifact }[];
  formalization_repository?: string;
  macros: Record<string, string>;      // of the current version, for rendering statements
  analysis?: PaperAnalysis;            // only when the author made it public
  conjectures: ConjectureFollowUp[] }; // only when the author made the analysis public

type TaskKind = "judge_escape" | "literature_check" | "formalize" | "probe";
type Task = { id: string; kind: TaskKind; target: { submission: string; claim: string };
  dedupe_key: string; title: string; contributors: string[];
  status: { state: "open" } | { state: "leased"; lease: { holder: string; mode: "own_agent" | "hosted"; until: string } }
        | { state: "submitted"; contribution: string } | { state: "done"; contribution: string } | { state: "closed"; reason: string };
  created_by: string; created_at: string; updated_at: string; revision: number };
type TaskContext = { task: Task; paper_title: string; abstract_text: string; claim: Claim;
  dependencies: Claim[]; repository?: string; pdf_public: boolean; nature: string;
  macros: Record<string, string> };
type NewContribution = { agent: { tool: string; model: string };
  output: { output: "judgement"; shape: "bind_only" | "content"; witnesses: string[]; rationale: string }
        | { output: "literature"; prior: PriorWork[]; searched: string[]; summary: string }
        | { output: "probe_note"; note: string }
        | { output: "pull_request"; url: string };
  tokens?: { input: number; output: number } };
type Contribution = { id: string; task: string; kind: TaskKind; contributor: string; mode: "own_agent" | "hosted";
  agent: { tool: string; model: string }; output: NewContribution["output"];
  tokens?: { input: number; output: number; metered: boolean };
  status: { state: "submitted" } | { state: "verified"; detail: string } | { state: "rejected"; reason: string };
  submitted_at: string; updated_at: string; revision: number };
type Credit = { contributor: string; verified: number; submitted: number; rejected: number;
  verified_formalizations: number; metered_tokens: number; reported_tokens: number };
type DonationGrant = { donor: string; monthly_cap: number; model: string; period: string; used: number;
  status: "active" | "paused" | "revoked"; created_at: string; updated_at: string; revision: number };
```

## Visibility

- A paper in draft, in review, not accepted or withdrawn is visible to its
  authors and to staff (editors, reviewer accounts, admins) only.
- When the author sets `open_to_contributors`, signed-in contributors can read
  each statement, its dependencies, and the paper's title and abstract through
  the task routes, while the paper is in review. Closing it closes the tasks.
- An accepted paper's statements, PDF and formalization badges are public. Its
  analysis and conjecture follow-ups are public only after the author chooses
  `public`.


## Referee rounds and sent feedback

A referee round reads exactly one `version` and `claims_revision`. The referee
recommendation is advice; it does not change S2/S3 adoption, `Policy::decide`,
acceptance, or formalization consent. Updates conflict when either input
revision changes. Settled steps (`done`, `failed`, `skipped`) are immutable.
Editors may start a new round only after the current round settles; reviewer
accounts begin a round idempotently for the current version and statement set.
These worker operations are internal application services, not HTTP routes.

Advice leaves `pending` only after the referee settles. It runs only for
`accept` or `minor_revision`; otherwise it is `skipped`, including a failed
referee. A letter leaves `pending` only after the referee and advice settle,
and a completed draft requires a completed referee report. A negative report
still gets a letter draft explaining the revision needed. A failed referee
skips the letter. A missing advisor records failed advisor steps. No Oracle
configuration means referee jobs complete without starting a round.

Staff (editors, reviewer accounts, admins) receive the full file. The submitter
and linked co-authors receive `rounds: []` and sent `letters` only. Other callers
receive 404, including contributors and readers of an accepted public paper.
A draft is never delivered automatically. Posting a letter stores its exact
subject, body and mathematical note as an in-app message. Body must contain
non-whitespace text. Body and note are each limited to 50,000 Unicode characters;
subject is limited to 300. `edited` compares all three fields with the current
round's draft; it is true when there is no draft. Letters can be sent for draft,
in-review, accepted and not-accepted papers; withdrawn papers conflict.

Digests use SHA-256 over two length-prefixed byte strings: the prompt's UTF-8
bytes and the PDF/source bytes, each preceded by its byte length as a big-endian
64-bit integer. Source bytes are canonical unpacked content in path order: each
UTF-8 path and file content carries the same length prefix. This binds names,
binary attachments and part boundaries as well as mathematical text.

The complete JSON vocabulary follows. Optional metadata may be absent or null;
timestamps are RFC 3339 UTC. `revision` fences aggregate writes, not paper
versions. `attempts` defaults to zero for older stored steps; new note, limits
and letter metadata fields have compatible defaults.

```ts
type RefereeFile = {
  id: string;                       // submission id
  rounds: RefereeRound[];            // chronological, staff only
  letters: FeedbackLetter[];         // chronological sent messages
  revision: number;                 // optimistic aggregate revision
};
type RefereeRound = {
  number: number;                   // 1-based, increases on restart
  version: number;                  // the paper version reviewed
  claims_revision: number;          // the confirmed statement set reviewed
  started_at: string;
  referee: Step<RefereeReport>;
  advice: Step<Advice>;
  formal: Step<FormalProbe>;        // rounds stored before it read as skipped
  letter: Step<LetterDraft>;
};
type Step<T> = {
  engine?: string | null;           // e.g. nyxid-oracle, codex-cli, openai-compatible
  model?: string | null;            // recorded model label, not UI attestation
  attempts: number;                 // external work starts; polls do not increment it
  client_ref?: string | null;       // durable Oracle idempotency identity
  input_digest?: string | null;     // SHA-256 hex binding prompt and PDF/source
  state: StepState<T>;
};
type StepState<T> =
  | { state: "pending" }
  | { state: "running"; task?: string | null; queue_position?: number | null; since: string }
  | { state: "done"; result: T; at: string }
  | { state: "failed"; reason: string; detail?: string | null; retryable: boolean; at: string }
  | { state: "skipped"; reason: string };
// A private formalization probe of up to two proposed statements. Staff only;
// never a recorded formalization. "compiled" means the file compiled against
// the named Lean and Mathlib with no sorry and only the listed axioms (a subset
// of propext, Classical.choice, Quot.sound); whether the Lean statement is the
// paper's statement is for a human to judge.
type FormalProbe = {
  toolchain: string;                // e.g. "leanprover/lean4:v4.33.0, Mathlib v4.33.0"
  attempts: {
    claim: string;
    outcome: "compiled" | "failed";
    theorem?: string | null;        // the checked declaration
    lean: string;                   // complete Lean source
    axioms: string[];
    note: string;                   // the prover's account of faithfulness
    log: string;                    // why a check failed
  }[];
  summary: string;
};
type RefereeReport = {
  recommendation?: "accept" | "minor_revision" | "major_revision" | "reject" | null;
  summary: string;
  strengths: string[];
  concerns: { claim?: string | null; severity: "major" | "minor"; issue: string }[];
  claims: {
    claim: string;                  // confirmed, proved statement id
    shape: "content" | "bind_only";
    witnesses: string[];            // explicit escape witnesses for content
    known?: string | null;          // source reported by referee, not verified literature
    note: string;
  }[];
  limits: string[];                 // unchecked work and sanitisation omissions
  text: string;                     // original, complete Oracle answer
};
type Advice = {
  summary: string;                  // also names dropped unusable outputs
  improvements: {
    claim?: string | null;          // null for a paper-wide suggestion
    kind: "gap" | "strengthen" | "generalize" | "computation" | "literature" | "exposition";
    suggestion: string;
    how_we_help: string;
    effort: "small" | "medium" | "large";
    status: "checked" | "proposed"; // advisor-reported evidence standing
    evidence: string;              // checked work/outcome; empty for proposed work
  }[];
  formalization: {
    claim: string;                  // proved statements only
    feasibility: "ready" | "needs_library" | "hard";
    mathlib: string[];              // notions and names, with uncertainty marked
    missing: string[];              // definitions/lemmas to build
    lean_sketch: string;            // Lean 4 statement sketch; may be empty, never a proof receipt
    plan: string;
    effort: "small" | "medium" | "large";
  }[];
};
type LetterDraft = {
  subject: string;
  body: string;                     // English editor draft, ordinary connected prose
  note: string;                     // longer mathematics, markdown with LaTeX; may be empty
};
type FeedbackLetter = {
  round?: number | null;            // current round when sent, or absent with no round
  subject: string;
  body: string;
  note: string;
  edited: boolean;                  // differs from the current round draft, or no draft exists
  sent_by: string;                  // editor's account
  sent_at: string;                  // in-app delivery timestamp
};
```

A running referee step has this JSON form; admission metadata can also exist on
`pending` while a submission with uncertain delivery is retried using the same
client reference and inputs:

```json
{
  "engine": "nyxid-oracle",
  "model": "chatgpt-6-pro",
  "attempts": 1,
  "client_ref": "wishpool:submission-id:r1:v1:c1",
  "input_digest": "<64 lowercase hex characters>",
  "state": {
    "state": "running",
    "task": "oracle-task-id",
    "queue_position": 2,
    "since": "2026-10-08T00:00:00Z"
  }
}
```

Unknown recommendations remain absent, with a limit recorded. Unknown/open
statement readings and concerns, duplicate readings and unusable proof
shapes are dropped and counted in `limits`; witnesses are never invented.
Unknown improvement enums/ids, checked improvements without evidence and
formalization candidates for open statements are dropped and named in
advice `summary`. Non-empty proposal evidence is cleared with an omission named
in the summary. Referee witnesses are preserved without truncation; readings
whose rationale cannot fit the domain's 10,000-character limit are dropped with
a limit, while the complete original answer remains in `text`. `checked` is an advisor's report of a computation or written
argument, not independent verification or a Lean proof. Referee concerns and
computational claims do not become verified evidence.

Oracle failures preserve `reason`, `detail` and `retryable`. The reasons
`infrastructure_retry_exhausted`, `prompt_delivery_uncertain`,
`usage_limit_reached` and `model_unavailable` are retryable, but the settled step
still requires an editor restart for another attempt. Transport failures and
429/5xx responses while submitting or polling defer the same job and task;
they do not consume job attempts. Completed unparseable answers retain the raw
text as a report without a recommendation, and the parse limitation is visible
to staff.

For example, this is a complete settled file as staff receive it. The digest
placeholders below stand for actual 64-character SHA-256 hex strings. The
referee's reading and advisor's evidence remain reported findings; the draft
and the editor-sent message are separate objects.

```json
{
  "id": "submission-id",
  "revision": 8,
  "rounds": [{
    "number": 1,
    "version": 1,
    "claims_revision": 1,
    "started_at": "2026-10-08T00:00:00Z",
    "referee": {
      "engine": "nyxid-oracle",
      "model": "chatgpt-6-pro",
      "attempts": 1,
      "client_ref": "wishpool:submission-id:r1:v1:c1",
      "input_digest": "<prompt-and-PDF digest>",
      "state": {
        "state": "done",
        "at": "2026-10-08T02:00:00Z",
        "result": {
          "recommendation": "minor_revision",
          "summary": "Clarify the compactness step in the main proof.",
          "strengths": ["The intermediate bound is explicit."],
          "concerns": [{"claim": "C1", "severity": "minor", "issue": "State the compactness hypothesis at the indicated step."}],
          "claims": [{"claim": "C1", "shape": "content", "witnesses": ["The intermediate uniform bound"], "known": null, "note": "This is the referee's reading of the proof."}],
          "limits": ["The numerical example was not reproduced."],
          "text": "The complete original referee answer, including its JSON block."
        }
      }
    },
    "advice": {
      "engine": "codex-cli",
      "model": "codex-default",
      "attempts": 1,
      "input_digest": "<prompt-and-source digest>",
      "state": {
        "state": "done",
        "at": "2026-10-08T02:20:00Z",
        "result": {
          "summary": "The compactness step can be made explicit.",
          "improvements": [{
            "claim": "C1", "kind": "exposition", "suggestion": "State the compactness hypothesis.",
            "how_we_help": "Provide a written explanation of the finite subcover step.",
            "effort": "small", "status": "proposed", "evidence": ""
          }],
          "formalization": [{
            "claim": "C1", "feasibility": "needs_library", "mathlib": ["Compactness; exact API uncertain"],
            "missing": ["The paper's specific bound"], "lean_sketch": "", "plan": "Define the bound before formalizing the estimate.",
            "effort": "medium"
          }]
        }
      }
    },
    "formal": {
      "engine": "codex-cli+lean",
      "model": "codex-default",
      "attempts": 1,
      "input_digest": "<targets-and-source digest>",
      "state": {
        "state": "done",
        "at": "2026-10-08T02:22:00Z",
        "result": {
          "toolchain": "leanprover/lean4:v4.33.0, Mathlib v4.33.0",
          "attempts": [{
            "claim": "C1", "outcome": "compiled", "theorem": "Wishpool.C1.main",
            "lean": "import Mathlib\n\nnamespace Wishpool.C1\n...", "axioms": ["propext", "Classical.choice", "Quot.sound"],
            "note": "States the theorem for finite gaps exactly as in the paper.", "log": ""
          }],
          "summary": "C1 formalized."
        }
      }
    },
    "letter": {
      "engine": "codex-cli",
      "model": "codex-default",
      "attempts": 1,
      "input_digest": "<letter-prompt-and-source digest>",
      "state": {
        "state": "done",
        "at": "2026-10-08T02:25:00Z",
        "result": {"subject": "Feedback on your paper", "body": "Dear A. Author,\n\nThe compactness step would benefit from an explicit hypothesis.", "note": "A longer mathematical note may go here."}
      }
    }
  }],
  "letters": [{
    "round": 1,
    "subject": "Feedback on your paper",
    "body": "Dear A. Author,\n\nPlease state the compactness hypothesis at the indicated step; an explicit explanation of the finite subcover argument would make the proof easier to follow.",
    "note": "A longer mathematical note may go here.",
    "edited": true,
    "sent_by": "editor-subject",
    "sent_at": "2026-10-08T03:00:00Z"
  }]
}
```

For an author the same file has `rounds: []`; `letters` and `revision` retain
the values above. A failed step's state is, for example:

```json
{"state":"failed","reason":"prompt_delivery_uncertain","detail":"page_crashed@waiting_response","retryable":true,"at":"2026-10-08T02:00:00Z"}
```
