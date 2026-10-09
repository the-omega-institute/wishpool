# Wishes

Status: **Proposed — not implemented**. Product changes require an owner decision before implementation or release.

This proposal makes an author-owned Wish the primary unit of mathematical work. Paper review and publication remain an optional venue service.
All new schemas, states, ports, routes, configuration names, authorization mechanisms and limits below are proposed work. Tables describing existing source contracts are explicitly identified as such; upstream extensions are asks, not existing APIs.
Wishpool citations are repository-relative, at commit **137d18c**. External citations identify **ChronoAIProject/cma@1c71725** and **ChronoAIProject/NyxID@dd8d45b**. Source evidence does not establish deployed behavior. Unverified assumptions are collected in §14; no Lean build or live hosted execution is claimed here.

## 0. Decisions and delivery scope

A Wish is a mathematician's intent to `formalize` a statement or `attack` a conjecture. An author can submit a statement directly or select one from their own paper. Execution does not require paper acceptance, a proved main result, S1 completion or an escape threshold.
The author pays for model inference with their own credentials. The venue checks candidates independently. An agent's completion message never changes a mathematical result.
The required chain is immutable input → author-confirmed target → bounded owner Run → untrusted candidate → venue VerificationReceipt.

| Phase | Deliverable | Enablement gate |
|---|---|---|
| 1 | Private standalone and paper-derived Wishes; trusted target elaboration; interactive author fidelity confirmation; own-agent SDK/MCP submission; remote verification, receipts and credit | Product contract, verification format, remote verifier and infrastructure funding agreed |
| 2 | One confirmed target, author payer, selected provider/model and one bounded CMA-hosted turn | NyxID payer pinning, usage and broker contracts; CMA app authority, delegated gateway, isolation and source transfer; numeric acceptance contract |
| 3 | Durable finite continuation, checkpoints, budgets, pause/resume and no-progress stopping | Phase-2 crash, revoke and budget journeys pass; separate continuation consent |

Phase 1 uses the author's own local coding agent and model tokens. It delivers the complete confirmation-to-evidence path. Hosted continuous execution is the remaining work in phases 2 and 3, gated by explicit upstream contracts.
A model may propose a statement before confirmation. Proof/attack admission requires a confirmed target. Owner Runs are separate from contributor Tasks; only the opted-in volunteer lane adds `TaskTarget::WishTarget`.
Reuse NyxID and CMA. Do not add a Wishpool model proxy, provider-key vault, coding-agent runtime or automatic multi-agent planner.

Decisions required before implementation:

1. Approve the Wish-first contract and three release phases, including own-agent execution in phase 1.
2. Adopt private defaults, author payer and a restricted proof format initially; broaden source support after verifier audit.
3. Select the first hosted provider, model, protocol and credential class. Start with a tested personal provider API credential; assess subscription credentials separately.
4. Allocate sandbox, verifier, storage and cache costs. Model BYOK does not pay these infrastructure costs.
5. Set turn, token, wall-time and no-progress caps and the corresponding user flow. Suggested limits are provisional.
6. Decide public text/artifact policy, coauthor fidelity authority and volunteer credit. Start with owner-only confirmation and separate publication consent.
7. Set the legacy import scope and cutoff. Existing attestations need author confirmation and venue rechecking before receiving a kernel-verification badge.

Upstream priorities: restricted NyxID broker/delegation provisioning; per-request owner/connection/class pinning without platform or organization fallback; attributed reservation/settlement and revoke bounds; reviewed CMA app authority; typed delegated gateway with secret custody, request fencing and isolation; bounded private source transfer; pinned Lean/Mathlib profiles and restore evidence. See §13.

## 1. Existing code and gaps

| Existing source contract | Gap for Wishes | Evidence |
|---|---|---|
| Paper venue; formalization follows acceptance | No standalone author-owned mathematical intent | `CLAUDE.md:8`, `CLAUDE.md:25` |
| `Submission` embeds formalization and conjectures | Publication and execution share a root | `api/crates/layer2-core/src/model/paper.rs:150`, `api/crates/layer2-core/src/model/paper.rs:182` |
| Formalization proposal requires Editor and acceptance; excludes open statements | Cannot directly take up an author's conjecture | `api/crates/layer2-core/src/app/formalization.rs:28`, `api/crates/layer2-core/src/app/formalization.rs:35` |
| Author approves a formalization item | Approval does not bind an exact Lean type, context and environment | `api/crates/layer2-core/src/app/formalization.rs:54`, `api/crates/layer2-core/src/model/formalization.rs:43` |
| S1 requires a proved main result | Conjecture-only input must bypass this paper gate | `api/crates/layer2-core/src/app/papers.rs:274` |
| Task target names a submission and claim | No standalone Wish target | `api/crates/layer2-core/src/model/task.rs:56` |
| Lease requires contributor opt-in; excludes paper author; formalize/probe require acceptance | Owner execution must have its own admission path | `api/crates/layer2-core/src/app/tasks.rs:251`, `api/crates/layer2-core/src/app/tasks.rs:257`, `api/crates/layer2-core/src/app/tasks.rs:262` |
| Formalize lease requires repository; SDK requires PR artifact | Bounded source candidates should submit directly | `api/crates/layer2-core/src/app/tasks.rs:276`, `sdk/contribute/src/rules.rs:43` |
| Hosted donation work covers judgement/literature; skips formalize/probe | Donated chat is not a Lean coding-agent loop | `api/crates/layer2-core/src/model/task.rs:40`, `api/crates/wishpool/src/donations.rs:377` |
| Donation path reads remaining quota, executes, then charges | Does not establish atomic pre-call reservation across processes | `api/crates/wishpool/src/donations.rs:203`, `api/crates/wishpool/src/donations.rs:395` |
| Editor verification validates submitted artifact/axiom metadata | Cannot mint new Wish kernel receipts | `api/crates/layer2-core/src/app/formalization.rs:142`, `api/crates/layer2-core/src/model/formalization.rs:104` |
| Venue compiles Lean and parses the first matching stdout axiom line | Independent-check precedent; insufficient hostile exact-target boundary | `api/crates/layer3-review/src/lean.rs:161`, `api/crates/layer3-review/src/lean.rs:186`, `api/crates/layer3-review/src/lean.rs:397` |
| Environment label can use `inputRev` or `unknown` | New evidence requires resolved full pins | `api/crates/layer3-review/src/lean.rs:139` |
| Bearer and session authentication collapse into person/roles; SDK can POST arbitrary paths | Owner identity does not establish human-only action authority | `api/crates/wishpool/src/auth/middleware.rs:57`, `api/crates/wishpool/src/auth/middleware.rs:78`, `api/crates/wishpool/src/auth/middleware.rs:105`, `api/crates/layer2-core/src/model/person.rs:61`, `sdk/contribute/src/client.rs:64` |

Retain the existing layering, L2 authorization, MongoDB revision fencing, GridFS and durable leases (`CLAUDE.md:72`, `CLAUDE.md:114`, `api/crates/layer2-core/src/ports.rs:1`).
The SDK is already a Bearer API client and stdio MCP server; extend it rather than introduce a runtime (`sdk/contribute/src/client.rs:28`, `sdk/contribute/src/client.rs:41`, `sdk/contribute/src/mcp.rs:1`).

## 2. Domain model and storage

All schemas in this section are proposed. Mutable roots carry `schema_version, revision, created_at, updated_at`; replacement requires the expected revision.
Immutable records carry `id, schema_version, created_at, content_digest`. Digests use SHA-256 with explicit algorithm and canonical-encoding version.
Identity is NyxID `issuer + sub`, or its Person mapping; email is not an owner or payer identity. Times are UTC. Lists are cursor-paginated.
Large input, source, transcript and logs use bounded BlobRefs rather than unbounded root fields. Existing BlobRef shape: `api/crates/layer2-core/src/model/paper.rs:44`.

### 2.1 Wish

- `id, owner_subject, coauthors[]`: owner controls fidelity, spending and disclosure, subject to §4.3 interactive authority.
- `title, objective: formalize|attack, lifecycle: draft|open|archived|withdrawn`.
- `current_version_id, current_target_id?`: mathematical text lives in WishVersion.
- `visibility: private|public, disclosure_revision, publication_manifest?`.
- `contributor_opt_in, contributor_consent_generation, processing_consent_id?`.
- `control_generation`: fence for mathematical edits, withdrawal and execution-wide cancellation.
- `result_projection {target_id, receipt_ids[], outcome}`: rebuildable cache governed by §3.3.
- `origin_refs[]`: bounded authorized provenance; does not automatically expose paper metadata.

### 2.2 WishVersion

- `id, wish_id, version_number, author_subject, previous_version_id?`.
- `informal_statement, assumptions[], definitions[], context, objective_snapshot`.
- `source_refs[] {blob_ref, excerpt_range?, excerpt_hash, rights_consent_id}`.
- `paper_origin? {submission_id, paper_version, claims_revision, claim_id/extracted_id, archive_hash}`.
- `input_digest`: mathematical text, definitions and source snapshots; `derivation_key?` records provenance.
- Mathematical edits create a version. Title, visibility and budget changes do not alter the mathematical digest.

### 2.3 TargetSpec

- `id, wish_id, wish_version_id, target_number, previous_target_id?`.
- `lean_target_source_ref, declaration_name, fully_quantified_type, trusted_prelude_ref`.
- `imports[], definitions_digest, dependency_closure[]`: pinned receipt/target/version or explicitly author-approved hypothesis.
- `environment {lean_version, compiler_digest, mathlib_full_commit, manifest_hash, image_digest, trusted_cache_digest}`.
- `semantic_digest`: elaborated proposition, definitions, universe/context, dependency closure and environment.
- `elaboration_evidence_id?` and immutable elaborated-expression hash; hashing pretty-printed source alone is insufficient.
- Separate `target_controls {target_id, state, revision, fidelity_receipt_id?, superseded_by?}` owns lifecycle.
- Semantic changes create a new TargetSpec; mark the prior control superseded. State changes do not rewrite the spec.

### 2.4 FidelityReceipt

- `id, wish_id, wish_version_id, target_id, semantic_digest, environment_digest`.
- `actor_subject, actor_role=owner, confirmed_at, confirmation_ui_version`.
- `display_manifest_hash`: exact displayed type, hypotheses, definitions, dependencies and explanation.
- `assertion=faithfully_states_my_intent, consent_action_id, challenge_id, target_control_revision, wish_revision`.
- Server-verified `auth_provenance=interactive_session, session_binding_hash, fresh_auth_at, action_evidence_id`.
- Append-only. Rejection/withdrawal adds control events and preserves evidence.
- MCP can propose but cannot confirm. Proposed challenge issuance and consumption are defined in §4.3; existing Caller does not carry this authority. Coauthor confirmation remains a product decision.

### 2.5 Run

- `id, wish_id, wish_version_id, target_id?, input_digest, semantic_digest?`.
- `purpose: statement|prove_or_attack, mode: own_agent|cma_hosted`.
- `owner_subject, funding_grant_id, grant_generation, wish_control_generation`.
- `execution_generation, revision, state, pause_reason?, stop_reason?, active_slot`.
- `bounds {max_turns, max_wall_seconds, max_input/output/total_tokens, max_unknown_reservation, no_progress_limit}`.
- `admission_snapshot {consent_id, model/provider/protocol, harness_revision, image/profile_pins}`.
- `remote {sandbox_id?, workspace_id?, agent_id?, cursor?, command_receipt_ids[]}`.
- `current_turn, next_due_at?, latest_checkpoint_ref?, mathematical_progress_fingerprint`.
- `result_ref?` references VerificationReceipt; an ended Run is not a solved Wish.
- A statement Run may precede confirmation and produce only a target proposal. Proof/attack requires a confirmed target.

### 2.6 FundingGrant

- `id, payer_subject, wish_allowlist[], purpose, custody_mode: external|broker_binding|sealed_refresh`.
- `credential_ref?`: binary-private custody reference; the domain stores no raw Bearer, refresh token or binding secret.
- `selected_service_id, connection_id, credential_owner, credential_class, provider_slug, model, protocol`.
- Explicit ordered `allowed_fallbacks[]`; phase 2 defaults empty and excludes platform, organization and operator fallback.
- `total_caps, per_turn_caps, concurrency_cap=1, expires_at, max_wall_seconds`.
- `status: active|paused|revoked, generation, consent_id, processing_disclosure_id`.
- `settled_usage, outstanding_reservations`: projections of a separate usage ledger. Initial owner lane requires payer=author.
- Phase-1 external grant records consent, bounds and self-report; it does not claim provider-enforced local quota.
- Sponsorship follows phase 3 under separate consent and never changes ownership.

### 2.7 VerificationReceipt

- `id, verification_job_id, verification_key, artifact_id/hash, run_id, turn_id, execution_generation`.
- `wish_id, wish_version_id, target_id, semantic_digest, fidelity_receipt_id, input_digest`.
- `checked_relation: exact_target|negation|subtarget|elaboration_only`.
- `checked_declaration, actual_type_expr_hash, expected_type_expr_hash, comparison_policy_version`.
- Toolchain, Mathlib, manifest, image, cache and checker digests; `dependency_receipt_ids[]`.
- `kernel_replay_pass, axiom_closure[], no_sorry, policy_version, outcome`.
- `outcome: proved|disproved|partial|target_elaborated|rejected|timeout|infrastructure_error`.
- `log_blob_ref, structured_evidence_hash, started_at, completed_at, checker_identity`.
- `provenance {human, agent, harness, runtime, reported_model, attested_model?}` distinguishes reports from attestations.
- Failure receipts are retained. Only policy-valid checker success receipts authorize solving.

### 2.8 Supporting records and indexes

| Proposed record | Contents |
|---|---|
| RunTurn | Immutable run/generation/turn, input hash, purpose, operation keys, admitted bounds, candidates, checkpoints and outcome |
| Artifact | Immutable owner, target/Run/generation, hash/size/format, source manifest, visibility and submitter |
| ExecutionJob | Mutable subject/generation/transition, lease token/until, availability and transport retry count |
| UsageReservation/Event | Upstream request, grant generation, token classes, reserved/settled/unknown state and authority evidence |
| Outbox | Fixed body hash, operation key, remote receipt and delivery state; persisted before side effects |
| CreditEvent | Receipt/actor/role effect; unique and replayable; no agent success-string authority |
| HumanActionChallenge | Nonce hash, owner/session binding, purpose, resources/digests/revisions, issued/expiry/consumption times; never agent context |
| ProcessingConsentManifest | Recipient/data/purpose/retention/cleanup/non-recall entries, version/hash and owner action evidence (§8) |

MongoDB remains authority; GridFS holds bytes (`CLAUDE.md:114`). Admission and finalization require transactions plus outbox and a replica set; verify deployment rather than assume it.
Every collection has unique `_id`; additional constraints and query indexes follow.

| Collection | Additional unique constraint or query | Purpose |
|---|---|---|
| `wishes` | Query `(owner_subject, updated_at, _id)` | Owner listing |
| `wish_versions` | `(wish_id, version_number)` | Immutable version |
| `wish_derivations` | Unique `(owner_subject, derivation_key)` → `wish_id` | Deduplicate across request keys and Wish IDs in creation transaction |
| `target_specs` | `(wish_id, target_number)` | Target identity |
| `target_controls` | `(target_id)` | One lifecycle authority |
| `fidelity_receipts` | `(target_id, actor_subject, semantic_digest, consent_action_id)` | Unique action evidence |
| `human_action_challenges` | `(nonce_hash)`; query `(session_binding_hash, expires_at)` | Atomic one-use consumption; Clock enforces expiry, not TTL alone |
| `wish_runs` | Partial `(wish_id, target_or_statement_slot)` where `active_slot=true` | One active Run per target in phases 1/2 |
| `run_turns` | `(run_id, execution_generation, turn_no)` | One admitted mathematical turn |
| `funding_grants` | Query `(payer_subject, status)` | No implicit cross-Wish donation routing |
| `credential_bindings` | `(issuer, client_id, payer_subject, purpose, binding_hash)` | Binary-only encrypted custody |
| `execution_jobs` | `(subject_id, generation, transition, step_key)` | Dispatch/poll/check deduplication |
| `artifacts` | `(run_id, generation, submission_key)` | Repeated upload returns prior artifact |
| `usage_reservations` | `(grant_id, generation, upstream_request_id)` | One reserve per request |
| `usage_events` | `(authority, upstream_request_id, event_id)` | Stream events; settle by final sequence |
| `verification_receipts` | `(verification_key)` | Artifact/target/environment/checker-policy replay |
| `credit_events` | `(receipt_id, actor_subject, credit_role)` | Exactly one credit effect |
| `outbox` | `(operation_key)` | No duplicate remote resource effect |
| `command_dedup` | `(actor_subject, route_scope, idempotency_key)` | Same body replay; altered body conflict |

Leases and ephemeral sessions may expire by TTL. Never TTL-delete receipts, credit or unresolved outbox operations.
CAS checks revisions and relevant generations. Unique indexes are a final race guard, not authorization.

## 3. State machines and fences

### 3.1 Target

```text
proposed → elaborated → awaiting_author → confirmed
   └→ elaboration_failed → proposed(new TargetSpec)
awaiting_author → rejected → proposed(new TargetSpec)
confirmed / awaiting_author → superseded(new semantic input/spec)
```

Elaboration establishes a complete `Prop` and context without requiring proof. Revisions after rejection create a spec, never alter definitions under an old digest.
Changes to proposition, hypotheses, notation/definitions, dependencies, imports or environment require confirmation again.
Proof-body updates, polling and budget reductions do not change the target. Environment upgrades are semantic changes for this contract.

### 3.2 Run

```text
requested → preparing → awaiting_statement → ended(statement_proposed)
                     └→ queued → dispatching → waiting_remote
                                                ↓
                       ended ← verifying ← artifact_ready
                         ↑         └→ queued(next finite turn; phase 3)
any nonterminal → paused(reason) → queued(re-admit under new generation)
any nonterminal → cancelled | failed | ended(inconclusive)
```

In phase 1, waiting means waiting for the author's own agent; the venue does not start or meter that local model.
Phase 2 ends after one hosted turn and verification. Automatic next turns start only in phase 3.
Pause reasons: `author_request, credentials, approval_required, budget, capacity, unknown_usage, transport, no_progress`.
Operational termination and mathematical outcome remain separate. Timeout, process failure or agent stop does not refute a conjecture.

### 3.3 Explicit result reducer

Proposed pure reducer inputs are current Wish/Target controls, trusted elaboration, FidelityReceipt, policy-valid VerificationReceipts and bounded Run history.
First filter evidence by target/version/environment/policy and current fences. Reduce separate fields `solve_evidence, readiness, checked_partial, operational_status`.

| Projection | Authority | Solved? |
|---|---|---|
| `unresolved` | No decisive valid receipt for current confirmed target | No |
| `formalized_target` / statement-ready | Trusted elaboration plus fidelity | No; statement is ready |
| `proved` | Current exact-target kernel receipt | Yes |
| `disproved` | Current negation kernel receipt, for attack objective | Yes |
| `partial` | Receipt for separately confirmed subtarget and explicit unresolved parent obligations | No |
| `inconclusive` | Bounded Run ended without decisive receipt | No |

Only valid receipts authorize proved/disproved/checked partial. Readiness comes from elaboration plus fidelity; operational inconclusive comes from Run history.
A failed Run never overwrites a valid solve receipt. Partial progress and inconclusive may coexist; display that the parent remains unresolved.
For formalize, a negation receipt is refutation evidence requiring owner decision, not completed formalization. For attack, proof or refutation settles the target.
Numerical counterexamples must become checked negation proofs. Failed search remains inconclusive.

### 3.4 Pause, resume, revoke, edit and withdrawal

| Command | Durable transition | Late work |
|---|---|---|
| Pause Run | CAS increments execution generation; pauses; releases future admission; outbox interrupt/stop | Old generation cannot spend or project current result; retain historical artifact |
| Resume Run | Recheck target, grant, consent, remaining bounds and resolved unknown usage; new generation | Rebind checkpoint/session upstream; never revive old capability |
| Revoke grant | Increment generation; revoked; block admission; enqueue affected stops | Reconcile in-flight usage; do not refund uncertain spend |
| Mathematical edit | New WishVersion; increment Wish control generation; supersede target; fence active Runs | Old receipt remains historical evidence, not current solve |
| Withdraw Wish | Withdrawn; increment control generation; stop new Runs/contributor access | Preserve evidence; public withdrawal follows disclosure policy |
| Disclosure edit | Increment disclosure revision; recheck read authorization | Does not change mathematical digest or authorize execution |

Check Wish, Run and grant fences at each model admission, not only at finalization. Completion checks lease token, Run revision/generation, Wish control generation, target/input digests and grant generation.
Cross-aggregate cleanup may be asynchronous; admission must synchronously fail closed on current root fences.
Execution and grant fences apply to admission and finalization only. A receipt already committed for the current target is not invalidated by a later pause, revoke or Run end; only a target supersession, environment or axiom-policy change, or withdrawal changes what it establishes.
Remote in-flight computation cannot be recalled. Upstream stop latency and residual consumption require published bounds. In phase 1, venue revocation stops admission/projection, not the author's local spending.

## 4. Paper derivation and fidelity

### 4.1 Derive a Wish

1. Author uploads their LaTeX or enters a statement. Store source rights and AI-use disclosure separately.
2. Paper extraction displays all statements, including conjectures/questions, before S1 or acceptance.
3. Author selects excerpt, intent, definitions/context and dependencies through proposed `POST /submissions/{id}/wishes`.
4. L2 verifies author authority and pinned paper version/archive hash, copying authorized context into immutable WishVersion.
5. Canonical derivation key is submission + paper version + claim/excerpt hash + intent. A unique `(owner_subject, derivation_key) → wish_id` mapping is written in the creation transaction. Different request keys or proposed Wish IDs still return the same Wish; roll back orphan versions/blob references on conflict.
6. Later paper versions show a diff. Author adoption creates a WishVersion and requires a new target/confirmation.

Extraction and S1 are separate existing steps (`api/crates/layer2-core/src/app/papers.rs:38`, `api/crates/layer2-core/src/app/papers.rs:274`). Retain extraction without the main-result gate.
Existing `claims_from` truncates statement bodies at 40,000 characters (`api/crates/layer2-core/src/app/papers.rs:65`). Proposed extraction metadata adds `extraction_truncated, original_char_count, excerpt_range/hash`.
Recover the exact excerpt from the pinned full archive. Require explicit completion and renewed preview, or refuse derivation from truncated input. Never confirm silently incomplete text.
Retain `.tex/.zip/.tar.gz` and archive/TeX limits initially (`CLAUDE.md:14`, `CLAUDE.md:129`); PDF/OCR follows later.
Standalone intake does not implicitly schedule paper review, Oracle, advisor or model extraction. Rule-based intake can use no model tokens.
Draft, not-accepted and withdrawn papers may supply owner Wishes. Source visibility constrains disclosure, not private mathematical work.

### 4.2 Confirmation view

Display the original informal statement/context beside the venue-elaborated Lean type, expanded hypotheses and definitions.
Show all quantifiers, universe parameters, implicit assumptions, dependency receipts and explicit unproved hypotheses.
Natural-language explanation is auxiliary and labels its author/agent source. Highlight possible vacuity or weakening without substituting for the author's judgement.
Offer request revision or confirm that the complete statement faithfully states the author's intent. Confirmation binds target, semantic/display/environment digests and revisions.
Processing permission, fidelity and publication are separate actions. Venue elaboration precedes confirmation even when a local agent proposed the target.
Third-party clients receive an owner confirmation URL and pending state; a human opens it in the owner browser session. The URL conveys no confirmation authority or nonce.
MCP omits automatic confirmation, and L2 also refuses direct owner Bearer POST confirmation (§4.3). Pending confirmation blocks proof/attack admission.
Stale/expired confirmation produces a conflict and renewed view. Agent claims, paper S1 and prior approval do not satisfy Lean fidelity.
A candidate must preserve the confirmed context. Unproved dependencies are explicit author-approved hypotheses or are refused; never import them silently as facts.
Existing author approval and prover notes are precedents only (`api/crates/layer2-core/src/app/formalization.rs:54`, `api/crates/layer3-review/src/lean.rs:336`).

### 4.3 Interactive owner authority — proposed work

Current middleware merges Bearer and cookie identity into the same Caller, which contains only person/roles (`api/crates/wishpool/src/auth/middleware.rs:57`, `api/crates/wishpool/src/auth/middleware.rs:78`, `api/crates/wishpool/src/auth/middleware.rs:105`, `api/crates/layer2-core/src/model/person.rs:61`). The SDK can POST arbitrary paths (`sdk/contribute/src/client.rs:64`). Tool omission cannot enforce the human gate.

Proposed `Caller.auth_provenance = bearer | interactive_session` carries verified session binding, authentication freshness and action evidence into L2.
The binary authentication adapter verifies the NyxID login/session and origin/CSRF evidence. L1 forwards trusted Caller evidence; JSON fields or client headers cannot assert provenance. L2 makes authorization decisions without doing HTTP/token verification or I/O.
Fidelity confirm, hosted spend-consent acceptance and publish require owner `interactive_session`. Refuse machine Bearers even for the owner/admin, with a cookie also present, or with a copied valid challenge. Never downgrade a Bearer request to cookie authority.
Issue this authority only through owner browser OIDC login, fresh authentication and the explicit displayed action. An SDK Bearer cannot mint or exchange for it; no agent-held confirmation scope is proposed.
If the current OIDC integration cannot establish authentication freshness, require login again rather than accept a client timestamp. New credential/scoping support is proposed work, with any provider-specific support an upstream ask.

1. Third-party client receives the proposed confirmation URL. Human opens it, signs in or refreshes authentication; server renders the current complete resource/revision display manifest.
2. Proposed challenge issuance is interactive-session-only, with verified origin/CSRF. Suggested fresh-auth maximum is 10 minutes and challenge TTL 5 minutes; both require approval and use L2 Clock.
3. Server generates a cryptographically random nonce and stores its hash, bound to `owner_subject, session_binding_hash, purpose=fidelity, wish_id, wish_version_id, target_id, semantic_digest, display_manifest_hash, environment_digest, wish_revision, target_control_revision, issued_at, expires_at`.
4. Deliver nonce only to the interactive page, never URL, SDK/MCP context, events or logs. Human explicit action submits nonce and expected bindings. Validate the trusted issuance record, not a caller-supplied `consent_action_id`.
5. Proposed `HumanActionUnitOfWork` atomically checks session/owner/freshness, purpose/bindings, expiry, unused nonce and current awaiting-author revisions/digests; consumes challenge, appends FidelityReceipt and CAS-updates Target control in one transaction. Conflicts create no receipt; a race permits one success.
6. Refuse expired, reused, stale or cross-session/owner/purpose challenges. Redisplay and issue a new challenge. Retried human actions read their existing receipt; a consumed challenge POST is refused rather than accepted by generic idempotency replay.
7. Hosted spend-consent and publication use separate challenge purposes with the same authority. Spend binds grant/payer/connection/model/caps/expiry/generation plus processing-manifest hash/revision. Publication binds complete disclosure-manifest hash and Wish/disclosure revisions. Expanded spend scope or newly public content requires confirmation again.
8. OAuth callback or processing consent alone does not activate a hosted grant. PATCH visibility, grant resume, Run creation and aliases cannot bypass acceptance/publish. Resume stays within existing consent and remaining bounds.

Session secrecy, trusted login and origin/CSRF checks define this boundary; possession of a stolen browser session remains a credential-compromise risk. This mechanism does not claim to detect arbitrary browser automation.
Caller provenance, challenge port/store/DTO/routes and atomic action enforcement are unimplemented and require direct REST acceptance tests before release.

## 5. Execution, durability and budgets

### 5.1 Phase 1: own agent and SDK/MCP

Author configures their own coding agent, subscription or API connection. Provider credentials stay local or in NyxID.
SDK holds only a Wishpool API Bearer without interactive fidelity/spend/publication authority. It uploads no raw model credential.
Existing MCP supports task list/lease/context/submit/release; preserve these volunteer tools and add the owner lane (`sdk/contribute/src/mcp.rs:13`, `sdk/contribute/src/client.rs:80`).
Journey: create/derive Wish → context → target proposal → trusted elaboration → interactive human confirmation → own-agent Run → bounded local candidate → asynchronous venue verification → receipt/diagnostics.
Submit source artifacts without repository, PR or merge; repository/full commit may remain provenance.
Phase-1 Run records authorized work and candidates, with no contributor lease, CMA control OAuth or hosted compute grant. Usage/model remain self-reported with no metered badge.
Venue cannot enforce local inference spending. Bounds are a consent/submission contract. The author drives repeated local turns; venue schedules elaboration and verification only.

### 5.2 Phase 2: one bounded hosted turn

The table lists existing CMA lifecycle APIs. New app, model, budget and data DTOs require upstream publication before use; do not inject unknown fields.
Current CMA closed DTOs reject unknown fields and cap bodies at 2 MiB (`ChronoAIProject/cma@1c71725:docs/API_REST.md:69`). Pin template/profile references, selected connection and control authority in the admission snapshot.

| Step | Existing method/endpoint and body, or explicit ask | Evidence/result |
|---|---|---|
| 0 | Provision reviewed app authority (ask); same-subject user `/auth/login` setup | Renewable-owner prerequisite: `ChronoAIProject/cma@1c71725:docs/API_REST.md:34` |
| 1 | `POST /api/v1/agent-profiles` `{name,description?,content}`; publish `POST /api/v1/agent-profiles/{id}/revisions` `{draft_version}` | `ChronoAIProject/cma@1c71725:docs/API_REST.md:475`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1045` |
| 2 | `POST /api/v1/workspace-profiles`; publish `/revisions`; content includes `location:{type:"directory",name}`, reviewed setup and agent references | `ChronoAIProject/cma@1c71725:docs/API_REST.md:511`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1309` |
| 3 | Create/publish sandbox profile; retain actual `{id,revision}`; delegated gateway mode is an ask | Existing modes/routes: `ChronoAIProject/cma@1c71725:docs/API_REST.md:517`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1171` |
| 4 | `POST /api/v1/sandboxes` `{sandbox_profile:{id,revision},name?,trust_confirmed?,commands_digest?}` | `ChronoAIProject/cma@1c71725:docs/API_REST.md:537`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1241` |
| 5 | GET sandbox detail/events until ready; `POST /api/v1/sandboxes/{id}/workspaces` `{workspace_profile:{id,revision},trust_confirmed?,commands_digest?}` | `ChronoAIProject/cma@1c71725:docs/API_REST.md:539`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1267` |
| 6 | GET workspace detail until ready; bounded private confirmed-context import is an ask | Detail: `ChronoAIProject/cma@1c71725:docs/API_REST.md:1268` |
| 7 | `POST /api/v1/workspaces/{id}/agents` `{agent_profile:{id,revision},title,first_message}` | `ChronoAIProject/cma@1c71725:docs/API_REST.md:543`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1068` |
| 8 | GET `/api/v1/agents/{id}`, `/events?after=<cursor>&limit=...`, `/transcript`; persist cursor and first-message state | Admission is not completion: `ChronoAIProject/cma@1c71725:docs/API_REST.md:553`; reads: `ChronoAIProject/cma@1c71725:docs/API_REST.md:1059`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1061`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1067` |
| 9 | Bounded worktree export (ask), or bounded transcript JSON, yields untrusted candidate/checkpoint; GET `/agents/{id}/usage` is diagnostic | Usage: `ChronoAIProject/cma@1c71725:docs/API_REST.md:1084` |
| 10 | Venue independently verifies; NyxID reconciles authoritative usage; Run ends | Proposed domain flow, no next turn |
| Control | `POST /api/v1/agents/{id}/interrupt` without body; `/stop` with `{}` | `ChronoAIProject/cma@1c71725:docs/API_REST.md:70`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1065`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1076` |
| Lifecycle | `POST /api/v1/sandboxes/{id}/commands` `{command:"pause"\|"revive"\|"retire",expected_revision}`; GET command receipt then sandbox readiness | `ChronoAIProject/cma@1c71725:docs/API_REST.md:565`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1248` |

Use stable `Idempotency-Key` and identical exact bodies; never guess profile IDs/revisions (`ChronoAIProject/cma@1c71725:docs/API_REST.md:390`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:475`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:482`).
If complete context exceeds text limits, pause until transfer exists; do not truncate hypotheses. Caller-base64 artifact registration is not worktree export; whole-home snapshot is owner-only (`ChronoAIProject/cma@1c71725:docs/API_REST.md:1214`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1234`).
Use a disposable sandbox per owner/security grant. Validate the processing/retention manifest (§8) before transferring private context.

### 5.3 Phase 3: finite continuation

Wishpool schedules finite turns; CMA supplies sessions/runtime. This proposal does not depend on a CMA scheduler; repository-wide absence of one remains unverified.
Continue a session with existing `POST /api/v1/agents/{id}/input` `{text}` and stable key, not a new agent (`ChronoAIProject/cma@1c71725:api/crates/layer1-public/src/rest/agent_openapi_shapes.rs:31`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1075`).
Turn input includes target digest, venue diagnostics, bounded frontier, checked lemmas and checkpoints. Every turn has stop conditions; budget/consent expiry pauses. A partial lemma cannot replace the parent proposition.
Proposed no-progress policy pauses after three turns without policy-approved checked progress. Normalize alpha/format/comment/path/timestamp changes and cosmetic diagnostic text/order. A new source hash or diagnostic string alone does not reset the counter.
Record exploration separately. New confirmed subtarget receipts or trusted canonical obligation discharge may reset progress; model self-assessment and narrative changes do not. Hard token/turn/deadline caps always apply.
Suggested bounds: 10 turns, 20 minutes per turn, four hours per Run; token caps depend on model upper bounds and owner approval. Current CMA support for enforceable limits is not claimed.
Resume shows used/reserved budget, checkpoint and stop reason; no implicit expansion. Owner explicitly resumes after no-progress. Cumulative allowances do not reset; an absolute admission deadline prevents indefinite pause/resume extension.
Lost sessions require reconciliation/stop before replacement and recorded checkpoint restoration. Never rebill an unresolved model request automatically. Provider switches require accepted protocol adapters; incompatible model uses a new session with mathematical checkpoint.

### 5.4 Unit of work, leases and idempotency

Proposed pure L2 `ExecutionUnitOfWork.admit_turn/finalize_receipt` command port expresses the cross-root boundary. Commands carry read sets, expected revisions/generations, immutable IDs/hashes and effect plans.
The binary implements MongoDB transactions/outbox; memory implementation provides an atomic fake. Individual store replacements cannot substitute for this port.

| Proposed command | Read set and fences | Atomic writes |
|---|---|---|
| `admit_turn` | Wish, Target, fidelity, Run, grant, consents, reservations and dedup; active slot, remaining bounds and all current generations/revisions | Reservation, immutable RunTurn, Run CAS, Outbox and dedup |
| `finalize_receipt` | Job lease, Artifact, Target, fidelity, Wish, Run, grant, policy and verification key; current semantic/input/environment and authority fences | Append-only receipt, Run transition, reducer projection, unique credit and outbox; stale completion writes history only, with zero current effects |
| `HumanActionUnitOfWork` | Verified session/action evidence, unused challenge and resource revisions/digests (§4.3) | Consume challenge plus fidelity/control, or grant acceptance, or disclosure publication |

On transaction conflict, reread the transaction snapshot and rerun L2 authorization and pure transition validation. Bound transient retries with original operation/body hash; unknown commit is reconciled by key. No CMA, NyxID or compiler call occurs inside a transaction.
After admission commit, a short lease runs one dispatch/poll/check transition. Long sessions hold no MongoDB lease.
Operation key is `run_id:generation:turn:operation`; persist exact body hash, remote IDs and command receipts. Same key/changed body conflicts. Human one-use actions follow §4.3 rather than generic replay acceptance.
Ambiguous response replays the same key/body or queries receipt. Unreconciled transport enters `paused(transport)` without a second paid turn.
Existing jobs use submission/kind identity, 30-minute lease and fenced completion/defer (`api/crates/wishpool/src/store/jobs.rs:19`, `api/crates/wishpool/src/store/jobs.rs:21`, `api/crates/wishpool/src/store/jobs.rs:93`). Generalize subject/step; defer refunds claim attempt and can support polling (`api/crates/wishpool/src/store/jobs.rs:101`). Separate transport retries from mathematical turns.
Definitive CMA setup refusal may use a new key after login; ambiguous failure retains the old key/body (`ChronoAIProject/cma@1c71725:docs/API_REST.md:63`).
Competing schedulers rely on transaction/CAS/unique active slot. Outbox delivery may repeat; effects deduplicate by operation/receipt. Without cross-root atomicity, block admission and solved publication rather than expose half-finalized results.

### 5.5 Accounting authority

Wishpool owns grant/turn envelopes; each actual model request is admitted by trusted CMA loopback and NyxID under the same generations.
Reserve for input, bounded output/reasoning and cache semantics. Do not host a hard-cap model without a measured conservative upper bound.
Every request has upstream ID and Wish/Run/turn correlation; settle once. All outstanding concurrent reservations count against grant caps.
Missing terminal usage, stream loss and ambiguous dispatch retain conservative reservation and pause unknown usage until authoritative reconciliation. Revocation does not waive already admitted spend.
Keep CMA cumulative usage, own-agent self-report and NyxID authoritative usage separate; neither sum them nor substitute one for another.
Existing NyxID schema includes billing request, owner, selected connection, credential class and token breakdown, but the cited schema does not establish Wish/Run/OAuth-client attribution (`ChronoAIProject/NyxID@dd8d45b:backend/src/models/usage_meter.rs:229`, `ChronoAIProject/NyxID@dd8d45b:backend/src/models/usage_meter.rs:242`, `ChronoAIProject/NyxID@dd8d45b:backend/src/models/usage_meter.rs:252`). Reuse billing; do not infer platform-wide absence of app quotas.

## 6. NyxID credentials, payer pinning and revocation

### 6.1 Custody

Phase 1 needs no hosted compute OAuth. Phase 2 sends raw provider keys only to NyxID. Wishpool domain holds selection references; binary holds sealed broker handle or fallback refresh token.
CMA trusted control/loopback receives short delegated authority through reviewed secret delivery. Agent sandbox receives no NyxID access/refresh token, client secret or provider key.
The agent sees only an attenuated Run/generation model capability, with no alternative payer/model/service or direct provider egress.
Wishpool handles consent/admission/fences; CMA handles loopback and request caps; NyxID handles routing/custody/billing. Secret delivery, request fencing and revoke SLA are upstream contracts; do not substitute a Wishpool reverse proxy.
Only a reviewed binary-to-control provisioning channel may carry secrets. Never place Bearers in profiles, prompts, public DTOs, artifacts or logs. Current CMA environment variables are public configuration (`ChronoAIProject/cma@1c71725:docs/API_REST.md:517`).

### 6.2 Existing OAuth and delegation contracts

The proposed integration uses the following source-supported endpoints/scopes. Run attenuation and payer enforcement remain upstream asks.

| Step | Request/scopes | Source evidence |
|---|---|---|
| Sign-in | `GET /oauth/authorize`: code, `openid profile email`, S256 PKCE, state, nonce, registered redirect URI | `ChronoAIProject/NyxID@dd8d45b:docs/OIDC.md:156` |
| Incremental funding consent | Add `proxy urn:nyxid:scope:broker_binding`, `include_granted_scopes=true`, selected-service resource; create restricted compute binding if previous consent is broader | `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_client_service.rs:27`, `api/crates/wishpool/src/auth/nyxid.rs:155` |
| Service restriction | Repeated `resource=<NYX_BASE>/api/v1/proxy/s/<slug>`; live service IDs, not front-end selection alone | `ChronoAIProject/NyxID@dd8d45b:docs/OIDC.md:241` |
| Code redemption | `POST /oauth/token` form: `grant_type=authorization_code,code,redirect_uri,client_id,client_secret,code_verifier` | `ChronoAIProject/NyxID@dd8d45b:docs/OIDC.md:196` |
| Broker-only setup | Provision broker capability/scope; omit `offline_access`; seal `binding_id`; discard ephemeral setup access | `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:3`, `ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/oauth.rs:2380`, `ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/oauth.rs:2492` |
| Binding to direct access | Token endpoint: `grant_type=urn:ietf:params:oauth:grant-type:token-exchange,client_id,client_secret,subject_token=<binding_id>,subject_token_type=urn:nyxid:params:oauth:token-type:binding-id,scope=proxy` | `ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/oauth.rs:2679`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:35` |
| Direct to delegated | Token endpoint: direct access subject, `subject_token_type=urn:ietf:params:oauth:token-type:access_token,scope=llm:proxy`; admin `delegation_scopes=llm:proxy` | `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_client_service.rs:44`, `ChronoAIProject/NyxID@dd8d45b:docs/MCP_DELEGATION_FLOW.md:292` |
| Renewal | `POST /api/v1/delegation/refresh`, delegated Bearer; after expiry, repeat broker exchange then ordinary exchange | Consent recheck: `ChronoAIProject/NyxID@dd8d45b:docs/MCP_DELEGATION_FLOW.md:343` |
| Inference | Trusted CMA loopback calls `POST /api/v1/llm/gateway/v1/chat/completions` or accepted `/api/v1/llm/<provider>/v1/responses` protocol route | `ChronoAIProject/NyxID@dd8d45b:docs/MCP_DELEGATION_FLOW.md:330`, `ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:41`, `ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:50` |

Broker direct and ordinary delegated tokens last 300 seconds; retain service restrictions (`ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:71`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:997`, `ChronoAIProject/NyxID@dd8d45b:docs/MCP_DELEGATION_FLOW.md:310`).
Broker requested scope must be within stored scopes; `proxy` cannot expand service grant. Admin rollout policy is N1 (`ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:1435`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:47`).
Ordinary exchange inherits subject restrictions, forbids chained delegation and supplies no invented Run-scope parameter (`ChronoAIProject/NyxID@dd8d45b:backend/src/services/token_exchange_service.rs:191`, `ChronoAIProject/NyxID@dd8d45b:docs/MCP_DELEGATION_FLOW.md:320`).
App/service consent is an outer bound; upstream attenuated capability constrains Wish/Run/model/payer further. Responses handling exists but is not end-to-end compatibility evidence (`ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/llm_gateway.rs:1330`).

### 6.3 Sealed refresh fallback

Use only when broker rollout is unavailable and the author explicitly accepts fallback. Scope `openid profile email proxy offline_access`, still restricted to selected-service resource.
Binary seals refresh with TokenCipher. Partition custody by issuer/client/payer/purpose; serialize refresh with lease and revision CAS.
Token form `grant_type=refresh_token,refresh_token,client_id,client_secret` obtains direct access, then exchange `llm:proxy`.
Reuse cipher/client rather than unfenced donor replacement; broker-only mode must not accidentally request offline access (`api/crates/wishpool/src/donations.rs:39`, `api/crates/wishpool/src/donations.rs:117`, `api/crates/wishpool/src/auth/nyxid.rs:164`).
NyxID rotation has 120-second retry grace; later old-token reuse may revoke the family (`ChronoAIProject/NyxID@dd8d45b:docs/OIDC.md:234`). Proposed fencing/error policy reconciles timeout instead of blindly rotating or retrying indefinitely. Custody/admission failures fail closed; show fallback in UI and audit.

### 6.4 Payer and CMA control authority

`GET /api/v1/llm/status` and `/pools` are discovery, not credential-owner/class proof (`ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:59`, `ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:64`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/llm_gateway_service.rs:49`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/llm_gateway_service.rs:97`).
Constrain existing prefix/pool/provider fallback under the owner-pinned grant; no automatic pool fallback in phase 2 (`ChronoAIProject/NyxID@dd8d45b:backend/src/services/llm_gateway_service.rs:411`, `ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/llm_gateway.rs:870`, `ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/llm_gateway.rs:1447`).
Each request proves selected personal connection, payer=author, actual owner/class and model. Editing connection stops execution instead of silently changing class.
`credential_binding=platform` may spend the personal wallet using a master key, which does not satisfy author-owned provider tokens (`ChronoAIProject/NyxID@dd8d45b:docs/PLATFORM_KEYS_AND_INFERENCE.md:26`, `ChronoAIProject/NyxID@dd8d45b:docs/PLATFORM_KEYS_AND_INFERENCE.md:203`). Require upstream user-owned-only/no-platform/no-org/no-legacy-fallback enforcement.
Current independent CMA commands reject ordinary delegated/resource-scoped tokens, while service-bound agent-key verification exists (`ChronoAIProject/cma@1c71725:api/crates/cma/src/auth/bearer.rs:91`, `ChronoAIProject/cma@1c71725:api/crates/cma/src/auth/bearer.rs:111`, `ChronoAIProject/cma@1c71725:api/crates/cma/src/auth/agent_keys.rs:24`).
Bot Father delegation and CMAEG bindings have specialized authority, not generic Wishpool app authority (`ChronoAIProject/cma@1c71725:docs/API_REST.md:156`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:288`). Ask for reviewed owner-consented binding; do not retain unrestricted user impersonation or use compute tokens for commands.
Do not mint user agent keys through delegated `/api-keys`; the human router restrictions apply (`ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:2553`, `ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:2584`).

### 6.5 Revocation

Existing user routes delete client consent or broker binding: `DELETE /api/v1/users/me/consents/{client_id}` and `/broker-bindings/{binding_hash}` (`ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:2540`, `ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:582`, `ChronoAIProject/NyxID@dd8d45b:backend/src/routes.rs:595`).
Proposed handling of `oauth_broker_binding.revoked`: verify HMAC, deduplicate, increment grant generation, block model admission and enqueue CMA stop.
CAE delivery is best-effort; renewal/admission rechecks authority. Short capability TTL and maximum revoke window need upstream agreement (`ChronoAIProject/NyxID@dd8d45b:backend/src/services/cae_webhook_service.rs:4`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/cae_webhook_service.rs:36`).
Wishpool can revoke its grant. Full client-consent revocation affects other work under that consent; show the scope.

## 7. Independent verifier and receipt authority

### 7.1 Boundary and initial format

Existing `lean.rs` independently compiles and rejects some escape constructs (`api/crates/layer3-review/src/lean.rs:1`, `api/crates/layer3-review/src/lean.rs:359`). Extract a neutral proposed `LeanVerifier` and separate verification execution from model-driven formalization (`api/crates/layer3-review/src/lean.rs:85`, `api/crates/layer3-review/src/lean.rs:92`).
Initially accept only bounded proof terms or audited tactic fragments. The host generates the unique theorem wrapper and target import.
Disallow arbitrary commands/macros/elaborators, candidate plugins, unsafe/extern/implemented_by, native decision escapes, scripts and lakefile/config override. Apply restrictions to statement/prelude proposals too; definitions enter context only after trusted elaboration/audit.
Substring filtering is defense in depth, not a correctness boundary (`api/crates/layer3-review/src/lean.rs:360`). Broader source bundles require separate audit.

### 7.2 Proposed checking pipeline

1. Load immutable Artifact and target/fidelity references; check size, hash, format and generation snapshot.
2. Start a fresh remote, credential-free, network-off sandbox with read-only trusted dependencies.
3. Pin image/compiler/full Mathlib commit/complete manifest/cache; refuse `inputRev`, unknown pins and agent caches.
4. Discard uploaded oleans, lake directories/files, scripts and binaries. Rebuild only accepted source/proof with host wrapper.
5. Trusted elaboration inspector extracts actual type expression and definition/dependency hashes for controlled comparison to confirmed expression.
6. Comparator policy supports alpha-renaming and specified definitional equality, without context substitution.
7. Independent trusted checker/kernel replay rechecks proof expression, type and recursive dependency closure. Elaborator output alone is not authority.
8. Require recursive axioms ⊆ `{propext, Classical.choice, Quot.sound}`; reject sorryAx, admit, custom axioms and unchecked dependencies.
9. Trusted inspector/replay emits hashed structured evidence through a checker-owned channel. Candidate stdout/logs are diagnostics only.
10. Trusted composition checks evidence shape/version/policy and appends VerificationReceipt under checker identity.
11. Finalization transaction checks fences and projects result/credit. Stale evidence remains history with no current solve.

A LeanEval-style comparator is a proposed direction, not an installed dependency or a soundness claim. Structured JSON is not trusted when supplied by the candidate.
Receipt signing and database write authority stay outside untrusted compilation. Verification capability reads only its artifacts.
Remote CI must verify timeout, CPU/RAM/output/disk limits, non-root/seccomp, absent network/service-account access and crash isolation.

### 7.3 Outcomes

Formalize succeeds only with proof of the confirmed target; attack succeeds with target proof or negation proof.
Finite computation and counterexample narrative are findings until negation is kernel-checked. Partial lemmas have their own confirmed targets/receipts; no percentage-solved badge.
Elaboration plus fidelity gives readiness, not proof. Operational inconclusive derives from bounded Run history (§3.3).
Public REST/MCP may submit/request checking, never `compiled=true`, caller axioms, `verified=true` or receipt writes.
Replace first-match stdout parsing (`api/crates/layer3-review/src/lean.rs:186`, `api/crates/layer3-review/src/lean.rs:397`). L2 owns axiom policy and passes it to neutral L3; consolidate the existing duplicate constants (`api/crates/layer2-core/src/model/formalization.rs:40`, `api/crates/layer3-review/src/lean.rs:21`).
Agent reports, CMA status, editor metadata and paper acceptance cannot mint kernel success. Existing metadata remains `legacy_attested` until venue recheck (`api/crates/layer2-core/src/app/formalization.rs:151`, `api/crates/layer2-core/src/model/formalization.rs:104`).

## 8. Visibility, consent, retention and credit

Default Wish/source/target/Run/candidate/receipt/usage are private. Staff access follows assigned duties; foreign reads return `not_found`. This extends the paper precedent, not an existing Wish feature (`CLAUDE.md:82`).
Keep four consents separate: private processing, fidelity, volunteer access and publication. Hosted spend acceptance is a distinct budget authorization and binds processing consent.
Payer permission does not authorize disclosure. Public paper does not publish its Wish; public Wish does not publish private paper title/source.
Public projection uses an explicit publication manifest listing text/artifacts/receipt references and verified source disclosure rights. Publication, hosted spend acceptance and fidelity all require §4.3 interactive authority.

The proposed processing-consent manifest specifies, per recipient:

- Recipient: CMA control/sandbox, NyxID gateway, selected model provider and venue verifier; purpose, region and subprocessors.
- Data: exact excerpt/context/target, prompts, transcript, candidate and usage classes sent to that recipient.
- Retention: concrete duration, deletion API/acknowledgement, access roles, log redaction and backup expiry; distinguish venue evidence from CMA/provider content.
- Cleanup: end/cancel/withdraw stops new transfers and queues retirement/deletion of ephemeral sandbox/worktree/temp/export/transcript copies. Track acknowledgement, deadline and failures. Pause retention of checkpoints must be explicit.
- Non-recall: provider-delivered, downloaded and published copies cannot be recalled; immutable receipts and backups are not instantly deleted. Retain only evidence and redacted audit allowed by policy.

Unknown retention does not mean zero retention: keep hosting disabled until the manifest is known and confirmed. New recipients, data scope or retention require interactive consent again. Manifest hash/revision binds spend challenge and admission.
Contributor opt-out increments consent generation, closes WishTarget tasks/leases and new context access. Recheck authorization before delivering buffered chunks/events/pages; bytes already delivered cannot be recalled.
Owner work needs no editorial independence, while paper judgements retain different account/model rules (`CLAUDE.md:97`, `api/crates/layer2-core/src/app/tasks.rs:257`).
Receipt-derived credit names author, payer, human contributor, agent/harness and actually attested provider/model separately. Unique receipt/actor/role prevents duplicate effect; model usage is not mathematical credit.
Private progress reaches owner first. Public partial/inconclusive views distinguish findings from checked subtargets. Self-reported usage/model receives no metered/verified attribution badge (`api/crates/layer2-core/src/app/tasks.rs:319`, `CLAUDE.md:108`).

## 9. REST, SDK, layers and infrastructure

### 9.1 Proposed REST routes

All routes below are new proposed Wish APIs under `/api/v1`. Unsafe commands carry idempotency key and relevant expected revisions/digests/generations. Same key/body replays; changed body conflicts, except human challenge reuse must be refused (§4.3).
L2 owns author/payer/privacy/admission decisions, including provenance-based refusal. Route/command aliases must preserve the same authority.

| Proposed route | Meaning |
|---|---|
| `POST /wishes`; `GET /wishes`; `GET /wishes/{id}` | Standalone creation; authorized owner listing/view |
| `PATCH /wishes/{id}` | Metadata/contributor settings or narrower disclosure; expanded public scope requires publish; no mathematical edit |
| `POST /wishes/{id}/versions` | Immutable edit and old target/Run fences |
| `POST /submissions/{id}/wishes` | Pinned, deduplicated derivation without acceptance check |
| `POST /wishes/{id}/sources` | Bounded authorized BlobRefs, no hidden version change |
| `POST /wishes/{id}/targets`; `GET /targets/{id}` | Proposal/pins/asynchronous elaboration; exact type/context/evidence/confirmation view |
| `POST /targets/{id}/confirmation-challenges` | Interactive-only challenge issuance |
| `POST /targets/{id}/confirm`; `/reject` | Confirm requires verified interactive session and unused bound challenge; Bearer refused |
| `POST /wishes/{id}/funding-consents` | External consent or hosted initiation returning interactive URL; no hosted activation |
| `POST /funding-grants/{id}/spend-consent-challenges`; `/spend-consent` | Interactive-only issue/accept with grant and processing-manifest bindings |
| `POST /wishes/{id}/publication-challenges`; `/publish` | Interactive-only issue/accept with complete disclosure-manifest bindings |
| `GET /funding-grants/{id}`; `POST /funding-grants/{id}/pause\|resume\|revoke` | Payer control, generation and remaining/unknown usage; no consent expansion by resume |
| `POST /wishes/{id}/runs` | Mode/purpose/confirmed target/grant/bounds; owner lane |
| `GET /runs/{id}`; `/events`; `/turns`; `/usage`; `/context` | Bounded views/cursors/checkpoints; distinguish usage authorities |
| `POST /runs/{id}/pause\|resume\|cancel` | Expected generation/revision and outbox control |
| `POST /runs/{id}/artifacts` | Immutable candidate/target/generation/hash/submission key |
| `POST /artifacts/{id}/verify` | Idempotent check request without supplied success/axioms |
| `GET /verification-receipts/{id}` | Authorized immutable evidence |
| `POST /wishes/{id}/withdraw` | Fence admission, close tasks, retain history |

OAuth callback and CAE ingress are binary integration. Receipt minting and usage attestation are trusted background authority, never public write APIs.
Preserve `/tasks/...`; proposed tagged WishTarget has Wish/target/consent generation. Decode old PaperClaim submission/claim/revision compatibly. `lease_task` never creates owner Runs.

### 9.2 SDK/MCP

Preserve existing `list_tasks, lease_task, get_task_context, submit_contribution, release_task` (`sdk/contribute/src/mcp.rs:13`).
Proposed owner tools: `list_owned_wishes, create_wish, derive_wish, get_wish_context, propose_target, start_owner_run, get_run_context, submit_run_artifact, get_run_receipts, pause_run, resume_run`.
`start_owner_run` is own-agent only. Fidelity, hosted activation and publication return confirmation URLs for human browser action; SDK/MCP never receives action challenges. Direct Bearer REST requests are refused as well.
Add CLI owner commands, artifact upload and idempotency/revision flags. Conflicts/stale targets/pending confirmation should give recoverable next actions.
Outputs distinguish `statement_candidate|lean_candidate|finding|checkpoint` from verified receipt. Wish rules drop PR-only and accepted-paper requirements; paper review independence stays scoped to that lane.
SDK credentials are Wishpool API tokens; local agent/NyxID owns model credentials. Redact secrets and never log Bearer values.

### 9.3 Proposed layer responsibilities

| Layer | Owns | Excludes |
|---|---|---|
| L1 `layer1-public` | Wish/Run/Funding/Target/Artifact DTOs, pagination/error projection, forwarding trusted Caller evidence | Domain rules, provider HTTP, compiler |
| L2 `layer2-core` | Models/state/policy/authorization/reducer; proposed WishStore, TargetStore, RunStore, FundingStore, ReceiptStore, ExecutionQueue, BlobStore, SessionExecutor, LeanVerifier, ExecutionUnitOfWork and HumanActionUnitOfWork ports; verified provenance and challenge policy | I/O/provider SDKs/L3 imports |
| Neutral L3 | Proposed `layer3-execution` CMA profiles/commands/receipts and `layer3-lean` evidence adapter, or neutral review module | Domain imports and solve decisions |
| Binary `wishpool` | Port implementation/L3 mapping; Mongo/GridFS/transactions/outbox/jobs; provenance verification; NyxID custody/CAE; scheduling; policy/evidence composition | Generic model proxy/provider-key vault |
| `web`, `sdk/contribute` | Fidelity/consent/results UI and owner tools | Client-only authority |

Choose file split within the workspace contract. L3 cannot implement a trait requiring L2 dependency; binary holds adapter glue. L2 authorization and REST projection remain mandatory (`CLAUDE.md:72`, `CLAUDE.md:75`).

### 9.4 Infrastructure

Phase 1 adds a remote verifier runner with digest image, network denial, no deployment secrets, resource/storage limits and trusted evidence channel. Do not compile hostile Lean inside the existing TeX process/environment.
Current worker deployment uses one replica, shared environment references and 2Gi limit (`infra/worker/deployment.yaml:12`, `infra/worker/deployment.yaml:41`, `infra/worker/deployment.yaml:59`).
Proposed nonsecret config: `WISHPOOL_WISH_ENABLED, WISHPOOL_VERIFIER_IMAGE_DIGEST, WISHPOOL_VERIFIER_ENVIRONMENT_ID, WISHPOOL_VERIFIER_TIMEOUT_SECS, WISHPOOL_ARTIFACT_MAX_BYTES`.
Phase 2: `WISHPOOL_CMA_BASE_URL, WISHPOOL_CMA_*_PROFILE_REF, WISHPOOL_HOSTED_WISH_RUNS=false, WISHPOOL_FUNDING_CUSTODY_MODE`.
Secret names cover compute client secret, CAE HMAC and binding encryption; reuse TokenCipher key management and commit no values.
Phase 3 adds scheduling/no-progress/retention defaults and monitoring. Single TeX-volume ownership does not require one Run scheduler.
Gate replica-set transactions, indexes, GridFS retention, outbox lag/unknown usage, backup and restore. Remote CI prewarms Lean images; CMA/verifier use identical environment pins with separate trusted verifier cache.
Operator-controlled digest rotation has a CMA precedent (`ChronoAIProject/cma@1c71725:docs/Sandbox_Provider.md:12`). Research estimates of 4 CPU/16 GiB/20 GiB are not measured requirements (`ChronoAIProject/cma@1c71725:docs/CMA_Research.md:74`, `ChronoAIProject/cma@1c71725:docs/CMA_Research.md:512`).
Config, health/role/image inputs, infra manifests and contract changes share the same implementation change (`CLAUDE.md:141`). Extend fixed CI file/crate checks when adding infrastructure or L3 packages (§11.1).

## 10. Keep, remove and migrate

| Keep | Purpose/evidence |
|---|---|
| NyxID sign-in/session; L2 privacy; Mongo/GridFS/revisions | `CLAUDE.md:72`, `CLAUDE.md:114`, `CLAUDE.md:122`; extend with proposed provenance rather than collapse session/Bearer |
| Durable job claim/defer/retry | `api/crates/wishpool/src/store/jobs.rs:51`, `api/crates/wishpool/src/store/jobs.rs:101` |
| S0 extraction/archive limits/TeX isolation | `api/crates/layer2-core/src/app/papers.rs:38`, `CLAUDE.md:129`; add truncated-extraction/full-excerpt handling |
| Optional paper Policy/S2/S3/escape/referee/Oracle/WP record | Paper scope: `api/crates/layer2-core/src/policy.rs:1`, `CLAUDE.md:22`, `CLAUDE.md:165` |
| Volunteer Task/Contribution and SDK transport | `api/crates/layer2-core/src/model/task.rs:105`, `api/crates/layer2-core/src/model/task.rs:267`, `sdk/contribute/src/client.rs:41` |
| TokenCipher and independent compile/axiom-check direction | `api/crates/wishpool/src/donations.rs:39`, `api/crates/layer3-review/src/lean.rs:161` |

| Remove or retarget for Wish execution | Reason |
|---|---|
| Acceptance/escape/S1 main result/editor nomination/quorum prerequisites | Author mathematical work is independent (§1) |
| Contributor lease/opt-in/author exclusion for owner Runs | New owner aggregate; retain independent editorial lane |
| Mandatory repository/PR/merge | Direct artifact verification; old rule: `sdk/contribute/src/rules.rs:43` |
| Embedded Submission formalization/conjectures as new execution authority | Replace with linked authorized Wish projections; old fields: `api/crates/layer2-core/src/model/paper.rs:182` |
| Donor-any-paper routing for author Wishes | Explicit allowlist/payer; preserve old donation scope |
| Operator review/Oracle/local operator model as Wish fallback | No author payer guarantee; old config: `infra/api/secret.yaml:15`, `infra/api/configmap.yaml:43` |
| Stdout substring/editor metadata as new kernel success | Hardened receipts (§7) |

Oracle uses enrolled browser-account capacity without guaranteeing Wish author ownership; retain it only in optional paper review (`ChronoAIProject/NyxID@dd8d45b:docs/ORACLE_RELAY.md:3`, `ChronoAIProject/NyxID@dd8d45b:docs/ORACLE_RELAY.md:83`).

Proposed migration sequence:

1. Deploy versioned schemas/compatibility decoders with new admission flag off.
2. Owner opts in to import; stable source key maps submission/version/claim to Wish. Do not manufacture spending consent.
3. Imported statements/provenance are legacy; old approval is not fidelity. Require elaboration and awaiting-author confirmation.
4. Existing editor artifact is `legacy_attested` until exact-target remote recheck.
5. Paper pages read authorized linked receipts; immutable accepted records and one solve authority remain.
6. At cutoff, disable legacy mutations for migrated targets or translate explicitly into new commands. Avoid double authority.
7. Keep old Tasks readable. New mathematical volunteer tasks use WishTarget; existing review/donation rules retain their scope.
8. Update CLAUDE.md, README, ARCHITECTURE, CONTRIBUTE and UI together. Rollback disables admission and preserves receipts/history.

## 11. Phased implementation plan

These are proposed future changes. Product, cost, visibility and verifier-format decisions precede implementation; upstream evidence precedes phase-2 enablement.

| PR | Phase-1 scope | Behavioral acceptance |
|---|---|---|
| PR1 | Product/domain/persistence contracts; Wish/Version/Target/Fidelity/Run/Grant/Receipt; pure ports including unit of work and provenance; collections/indexes/CAS/outbox | Standalone without paper; semantic edits fence/supersede; active-slot race; schema roundtrip; no L2 I/O/dependency violations |
| PR2 | REST/web intake/derivation; source rights/privacy; restricted elaboration; interactive fidelity challenge and view | Draft/rejected/conjecture-only source; owner+derivation deduplication; stale/reused challenge rejection; full context; foreign not_found; owner Bearer bypass refused |
| PR3 | Neutral hardened verifier, remote isolation, exact-type comparator/replay/evidence, pins/cache/config/infra | Valid fixed fixtures pass; substitution/sorry/custom axiom/stdout spoof/cache poisoning fail; timeout never solves |
| PR4 | External grant, own-agent SDK/MCP/Run/context/artifacts/checking, recoverable jobs and self-report | Owner submits without contributor lease; unconfirmed proof refused; key/artifact replay; no model credential upload; Bearer cannot confirm/spend/publish/mint receipt |
| PR5 | Atomic result/credit, publication manifest/interactive action, legacy import, private projections, failure recovery and docs | Recoverable valid candidate eventually has exactly one receipt/current result/credit; stale work has zero current effects; no legacy badge promotion or private leakage; full phase-1 journey |

PR2 may use fake ports for domain/REST development; release requires PR3 real elaboration/checker. Missing real elaboration fails closed. PR4 cannot temporarily borrow contributor leases; PR5 cannot substitute metadata for evidence.
Phase-1 exit: real own-agent source → exact interactive author confirmation → venue receipt, independent of paper acceptance and with no venue model-token spend.
Phase 2: published upstream contracts/smoke → neutral CMA adapter/custody → durable one-turn transfer/admission → usage/revoke/crash journey. Exit requires actual owner/class attribution, renewal beyond 300 seconds, ambiguous-command replay, matching environment, venue recheck and all numeric §12.2 bounds.
Phase 3: finite continuation/checkpoint → conservative budgets/no-progress → outage restore/monitoring. Exit requires unattended finite turns with fault-tested pause/resume/revoke/edit fences. Additional providers and sponsorship follow later.
Independent architecture, quality and test acceptance remains a release gate; a design document does not establish release readiness.

### 11.1 PR-to-CI-gate mapping

Current CI uses standalone MongoDB rather than initializing a replica set (`.github/workflows/ci.yml:60`). Existing Lean integration test returns early without `WISHPOOL_TEST_LEAN_WORKSPACE` (`api/crates/layer3-review/src/lean.rs:458`). Ordinary green CI does not demonstrate transactions or real Lean verification.
CI hard-codes infra files and the two L3 crates (`.github/workflows/ci.yml:29`, `.github/workflows/ci.yml:42`); additions require updates to these checks.

| Change | Proposed additional required gate; retain CLAUDE.md §9 gates |
|---|---|
| PR1 | Disposable replica-set MongoDB job: transaction/index/rollback, owner+derivation uniqueness, active-slot race, unknown commit/recovery; deterministic Clock/failpoint tests |
| PR2 | REST/web fake-port session-versus-Bearer, challenge/CSRF and disclosure tests; unavailable real elaboration fails closed |
| PR3 | Mandatory remote pinned Lean workspace/verifier fixture job; set `WISHPOOL_TEST_LEAN_WORKSPACE`; missing prerequisites fail, never skip; new-L3 dependency and infra whitelist/config checks |
| PR4 | SDK and API gates; direct Bearer confirmation/spend/publication refusal; altered-body conflict; URL without capability; artifact submission/recovery |
| PR5 | Remote fixed-fixture complete own-agent journey; deterministic concurrency/crash, eventual-exactly-once, disclosure/cleanup/legacy/restore |
| Phases 2/3 | Upstream live protocol/renewal/revoke/resource journeys against published numeric contract; live-provider smoke separate from proof regression |

## 12. Acceptance tests

This is a proposed implementation test plan. Run heavy Lean and hosted checks in remote CI.
Existing gates: API fmt/clippy/test/deny; SDK fmt/clippy/test; web install/format/lint/types/test/build/audit; gitleaks, images and kubeconform (`CLAUDE.md:149`, `.github/workflows/ci.yml:77`, `.github/workflows/ci.yml:84`). SDK deny is not claimed as an existing gate.

- [ ] Standalone, conjecture-only, draft/not-accepted input creates/confirms/runs without S1/acceptance/repository prerequisite.
- [ ] Owner Run bypasses contributor lease; volunteer WishTarget requires opt-in; paper author exclusion remains in editorial lane.
- [ ] Unique owner+derivation mapping deduplicates across request keys/Wish IDs; altered request body conflicts separately; artifacts/Run keys replay without a second remote agent.
- [ ] Direct owner machine Bearer `POST /targets/{id}/confirm` is refused, even with owner/admin role, cookie or copied valid challenge. Tool omission alone is insufficient.
- [ ] Direct owner Bearer spend-consent/publish and aliases, including PATCH public visibility or grant expansion, are refused.
- [ ] Expired/stale target/revision/semantic/display/environment, reused and cross-owner/session/purpose challenges are refused. A fresh valid interactive owner confirmation succeeds once and records fidelity.
- [ ] Expired/revoked session, stale authentication, bad origin/CSRF and fabricated Caller provenance fail closed.
- [ ] Concurrent challenge consumption permits exactly one success; transaction failure does not consume it. Self-asserted provenance/action ID, S1 and old approval cannot substitute.
- [ ] Added hypotheses, vacuity, definitions/notation/import/dependency/environment changes show a diff and invalidate old fidelity.
- [ ] Valid exact-target proof and attack-negation fixtures yield receipts; numeric counterexample, search timeout and agent completion never solve.
- [ ] Wrong theorem/type substitution/parent partial cannot solve; unchecked dependencies are explicit hypotheses or refused.
- [ ] Sorry/admit/sorryAx/custom axiom/native escape/malicious macro/plugin/forged stdout or evidence are rejected.
- [ ] Uploaded oleans/lakefiles/cache/scripts, traversal/bombs and oversize artifacts never enter trusted dependencies.
- [ ] Checker cannot reach network, credentials or metadata service; resource exhaustion gives failure/timeout only; candidate cannot mint receipt.
- [ ] Corrupt cache, unknown/inputRev/full-commit mismatch fail closed; no verifier-cache poisoning by agent.
- [ ] Competing schedulers, expired/stale lease completions and named crash failpoints meet §12.1 exactly-once recovery assertions.
- [ ] Pause/edit/revoke/withdraw prevents old-generation spend/admission/current solve and preserves historical evidence.
- [ ] Resume revalidates target/grant/consent/bounds/unknown usage; no revived authority; cosmetic edits do not reset no-progress.
- [ ] Restricted broker selection, deletion/recreated service slug, no-chain delegation, rotation CAS and consent revocation are correct.
- [ ] Platform/org/legacy fallback, edited connection, wrong payer or unexpected model is refused at request admission.
- [ ] CMA control uses accepted app authority, not ordinary inference delegation; deployment actually supports the selected binding/key path.
- [ ] Selected-owner Responses/tools/stream journey exceeds 300 seconds with renewal; revoke/stop/residual consumption meets published numeric bounds.
- [ ] Each request reserves/settles once; input/output/reasoning/cache accounting is distinct; missing final usage retains reservation and pauses.
- [ ] Per-request/turn/Run caps, absolute deadline and concurrency boundaries hold; ambiguous inference is not duplicated by retry.
- [ ] Ready/admitted/first-message-completed states differ correctly; observer disconnect never adds a turn; command receipt precedes readiness recheck.
- [ ] Pinned real Lean workspace/cache restore/resource peaks and private import/bounded export pass §12.2.
- [ ] Role/resource/disclosure matrix covers list/context/artifact/download/receipt/events/usage and public Wish with private paper origin.
- [ ] Processing, spending, fidelity, volunteer and publication consent remain distinct; retention/cleanup/non-recall behavior matches the manifest.
- [ ] Self-report and attested usage are not summed; receipt-derived credit is unique; legacy metadata is not venue verified.
- [ ] Remote phase-1 own-agent, phase-2 one-turn and phase-3 finite-continuation journeys have separate evidence; release acceptance and infra/index/backup gates pass.

### 12.1 Deterministic concurrency, privacy and recovery

Use existing L2 Clock (`api/crates/layer2-core/src/ports.rs:21`), controlled lease times and barriers. Do not depend on sleeps or a live model solving a theorem.
Proposed named failpoints: `after_admission_commit`, `after_remote_accept_before_id_persist`, `after_verifier_evidence_before_finalize`, `after_receipt_commit_before_ack`, `before_credit_delivery`.
Use fixed hashed proof/negation/adversarial fixtures, including positive alpha-renaming and definitional-equality cases and negative context/definition substitution.
Assert exactly one admitted effect and eventual exactly one receipt/current result/credit after recovery of a valid candidate. At-most-one alone can hide lost results. Stale work has zero current effects and preserved historical evidence; unresolved transport explicitly pauses without paid retry.
Test caps/deadlines immediately below, exactly at and above the bound; same-key altered-body conflict separately; resume preserves cumulative allowance.
Matrix roles are owner, authorized staff, active/revoked volunteer, stranger and public reader. Matrix resources are list, context, artifact, download, receipt, events and usage, with per-resource disclosure manifest.
Revoke while download/stream is buffered; reauthorize each chunk/page/event before delivery. Denied responses and public logs contain no private identifiers/text. Already delivered bytes are not recalled.
Test truncated-excerpt refusal/completion, processing manifest revision, cleanup acknowledgement/failure and retention expiry.

### 12.2 Numeric phase-2 acceptance contract

Proposed requirement: keep the hosted flag false until owner/upstream publish a versioned manifest with concrete numeric bounds for every row below. TBD or qualitative wording fails the gate. Set bounds before measuring, not from the result afterward.

| Required bounds/invariants | Evidence and pass rule |
|---|---|
| `stop_deadline_ms, revoke_deadline_ms, max_inflight_requests, residual_input/output/reasoning_tokens, residual_cost` | Stop acknowledgement/capability revocation/last-request times plus authoritative usage; each measurement ≤ agreed ceiling |
| `max_cpu, max_ram_bytes, max_disk_bytes, max_artifact_bytes, max_export_bytes, max_turn_seconds` | Peak measurements, timeout/kill and oversized-output refusal, for normal and hostile fixtures; all ceilings hold |
| `cleanup_deadline_seconds, recipient_retention_days, backup_expiry_days` | Per-recipient deletion/cleanup acknowledgement and retained manifest satisfy consent |
| Exact restored target/checkpoint/session invariants | Target/semantic/environment/checkpoint hashes, cursor, last completed turn and reservations match; new authority generation; no duplicate request |
| Zero unauthorized requests or private-source disclosures | Disabled fallback and hostile egress probes; every request matches author payer, selected connection/class/model |

Sanitized evidence records deployment/build, model/protocol, pins, Wish/Run/turn/generation, operation/body hash, request/connection references, credential owner/class, reservations/settlement/token classes, stop/revoke/cleanup times, resource peaks, restore digests and probe outcomes.
IDs and identity references belong in restricted or pseudonymous audit, not public logs. Exclude keys, tokens, private source and prompts. Missing evidence, unknown usage or an exceeded ceiling fails enablement.
Renewal beyond 300 seconds and a real Responses/tools/stream journey are necessary evidence, not substitutes for the remaining bounds.

## 13. Upstream asks

These are proposed integration asks, not new published upstream APIs. Find reusable contracts before adding new facilities.

| ID/team | Ask and required evidence | Blocks | Current source evidence |
|---|---|---|---|
| N1 NyxID | Confidential compute client; broker capability/scopes, restricted service resources, `delegation_scopes=llm:proxy`; renewal/incremental consent/CAE and tenant rollout contract | Phase 2 | `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_client_service.rs:27`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_client_service.rs:44`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/oauth_broker_service.rs:35`, `ChronoAIProject/NyxID@dd8d45b:docs/OIDC.md:241` |
| N2 NyxID | Nonsecret selected-connection resolver; execution-time payer/owner/class pinning; user-owned-only without platform/org/legacy fallback; edited/revoked connection refused with actual-owner evidence | Phase 2 | `ChronoAIProject/NyxID@dd8d45b:backend/src/services/llm_gateway_service.rs:49`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/llm_gateway_service.rs:97`, `ChronoAIProject/NyxID@dd8d45b:docs/PLATFORM_KEYS_AND_INFERENCE.md:26`, `ChronoAIProject/NyxID@dd8d45b:docs/PLATFORM_KEYS_AND_INFERENCE.md:203` |
| N3 NyxID | Reuse durable billing; app/Wish/Run/request attribution, reservation/cap/settlement/reconciliation; token classes, missing streams/retries, numeric residual spend/revoke bounds (§12.2) | Phase-2 bounded spend; phase-3 continuous budgets | `ChronoAIProject/NyxID@dd8d45b:backend/src/models/usage_meter.rs:229`, `ChronoAIProject/NyxID@dd8d45b:backend/src/models/usage_meter.rs:242`, `ChronoAIProject/NyxID@dd8d45b:backend/src/models/usage_meter.rs:264`; not an assertion of platform-wide absence |
| N4 Both | Real selected-author Responses/tools/stream journey, renewal >300 seconds, usage/cancel protocol and supported-model matrix; failure without managed fallback | Phase 2 | `ChronoAIProject/cma@1c71725:docs/CMA_Connection_Flow_Implementation.md:72`, `ChronoAIProject/NyxID@dd8d45b:backend/src/handlers/llm_gateway.rs:1330`, `ChronoAIProject/NyxID@dd8d45b:backend/src/services/llm_gateway_service.rs:411` |
| C1 CMA | Reviewed owner-consented Wishpool app binding; Run-scoped create/read/input/interrupt/stop/lifecycle authority, generation and renewal; no borrowing Bot Father identity | Phase-2 control | `ChronoAIProject/cma@1c71725:api/crates/cma/src/auth/bearer.rs:91`, `ChronoAIProject/cma@1c71725:api/crates/cma/src/auth/agent_keys.rs:24`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:156`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:288`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:34` |
| C2 CMA | Typed `delegated_gateway`, secret provisioning and loopback custody; no NyxID token in agent; per-request grant/Run fences, reserve/settle correlation and published DTOs | Phase-2 model path | `ChronoAIProject/cma@1c71725:docs/API_REST.md:517`, `ChronoAIProject/cma@1c71725:docs/CMA_Connection_Flow_Implementation.md:72`; proposed extensions |
| C3 CMA | Worktree isolation, denial of direct model/data exfiltration egress; numeric interrupt/stop/revoke latency and residual consumption; autonomous mode alone is not isolation | Phase 2; phase-3 stop | `ChronoAIProject/cma@1c71725:docs/API_REST.md:1065`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1076`; new isolation requirements need deployment evidence |
| C4 CMA | Bounded private import/worktree source export or audited capability channel; processing recipient/retention/deletion manifest; no whole-home export as candidate and no artifact registry as filesystem API | Phase-2 private data/results | `ChronoAIProject/cma@1c71725:docs/API_REST.md:511`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1214`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1234`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:1243` |
| C5 CMA | Digest-pinned Lean/Mathlib profiles/image/manifest/cache; real UID/worktree/read-only-root smoke, numeric resource bounds and exact checkpoint/session restore (§12.2) | Phase-2 Lean; phase-3 resume | `ChronoAIProject/cma@1c71725:docs/CMA_Research.md:74`, `ChronoAIProject/cma@1c71725:docs/CMA_Research.md:512`, `ChronoAIProject/cma@1c71725:docs/Sandbox_Provider.md:12` |
| N5 Both | Revocation/CAE revalidation, capability/fence SLA and unattended approval; distinguish invalid/expired/refused/dependency outage; publish numeric revoke acceptance | Phase-2 revoke; phase-3 unattended | `ChronoAIProject/NyxID@dd8d45b:backend/src/services/cae_webhook_service.rs:4`, `ChronoAIProject/NyxID@dd8d45b:docs/MCP_DELEGATION_FLOW.md:351`, `ChronoAIProject/cma@1c71725:docs/API_REST.md:27` |

Hosted execution stays disabled until these contracts are delivered and their acceptance journeys pass. Prioritize N1/N2/N3/C1/C2/C3 contracts and the N4 real journey. C4 may initially use a measured complete bounded text slice; larger context must not be truncated or disclosed.
Current own-credentials mode uses native device OAuth/provider routing; managed mode is a public service. Neither becomes delegated BYOK through renaming (`ChronoAIProject/cma@1c71725:docs/CMA_Connection_Flow_Implementation.md:35`, `ChronoAIProject/cma@1c71725:docs/CMA_Connection_Flow_Implementation.md:65`).

## 14. Unverified assumptions

Each item is **ASSUMED-UNVERIFIED** until its gate has evidence.

| ID | Assumption/risk | Required gate |
|---|---|---|
| A1 | Cited source matches deployed NyxID/CMA versions, broker flags, tenant permissions and service-bound introspection | Deployment parity and remote smoke before phase 2 |
| A2 | Generic CMA app binding, delegated gateway and payer-pinned request contracts are accepted and delivered | N1–N3/C1–C3; hosted flag off; no replacement Wishpool proxy |
| A3 | Selected-author Responses/tools/stream works throughout renewal and each provider has enforceable token upper bounds | N4; enable tested models/protocols only; no silent fallback |
| A4 | NyxID platform lacks existing per-app cap/attribution APIs; cited schema alone is insufficient to establish that absence | N3 searches reusable contracts; no duplicate billing/vault |
| A5 | CMA lacks any scheduler, supports long turns and all needed restoration cases | Wishpool finite scheduling does not depend on this; C5 measurements |
| A6 | Lean recipe CPU/RAM/disk/cache performance is adequate; research estimates are actual requirements | Remote measurements and cost decision; no local heavy build requirement |
| A7 | Restricted syntax, comparator and independent kernel replay form a sound hardened verifier | PR3 audit/adversarial fixtures/trusted channel; no kernel badge beforehand |
| A8 | Provider terms permit hosted automation, subscription reuse and third-party service/Oracle capacity | Separate provider/account/protocol product and terms assessment; source is not permission |
| A9 | Sandbox/verifier/storage funding or free allowance exists | Explicit owner cost policy; separate infrastructure from model BYOK |
| A10 | Owner approves phase-1 own-agent release, defaults, coauthor and credit policy | §0 decisions; design approval does not replace activation consent |
| A11 | Live MongoDB replica set, cross-root transactions and backup restore are ready | PR1 replica-set/rollback/recovery gate; no admission without atomicity |
| A12 | Independent architecture/quality/test release acceptance has passed | Obtain release acceptance; this proposal does not claim it |
| A13 | Interactive Caller provenance, fresh browser challenges and atomic fidelity/spend/publication enforcement exist | §4.3, PR1–PR4 direct REST gates; current Caller cannot express them |
| A14 | Numeric phase-2 bounds and recipient/data/retention manifest are agreed and met | §8/§12.2; missing values or measurements keep hosting disabled |

Hosted authority/budget contracts and verifier soundness remain the largest implementation risks and explicit release gates. Model BYOK forbids operator-funded user inference; verification uses no model but still needs CPU/storage funding.
Consent must state residual in-flight consumption and the limits of recalling delivered private data and historical receipts. Product changes, upstream delivery and live execution require the evidence specified above.
