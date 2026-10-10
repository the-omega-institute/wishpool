# wishpool

A venue for mathematicians' own papers, short notes and conjectures, with one
submission entry and one pipeline: GPT Pro referee → Codex audit → automatic
decision → immediate public display → private letter → Lean.

- **Submit.** Upload `.tex`, `.zip` or `.tar.gz` for any kind. Conjectures also
  offer typed title, mathematical statement with TeX preview, background and
  origin; the typed form becomes an ordinary stored LaTeX source and PDF.
- **Confirm.** Authors confirm the extracted statements and dependencies.
  Paper/note main results are proved; conjectures have a main open claim.
- **Review.** Short notes follow the paper pipeline. Papers pass on audited new
  content or a sourced open-problem settlement. Conjectures are displayed when a
  main claim is audited well-posed, open and mathematically new if proved.
  Model recommendations remain feedback; the pure policy applies the decision.
- **Display.** Every kind shares `WP-<year>-NNNN`. “Make public after acceptance”
  defaults to checked and can be changed later. Public details show statements,
  dependencies, new-content witness lemmas and verified Lean proofs. Reviews,
  correctness labels, comments, reports, advice and letters stay author/staff-only.
  Private/undecided records expose only title, authors, kind and record id.
- **Solve.** Download an author-confirmed `Target.lean`, prove its proposition or
  its negation in Lean, and submit the solution. A separate credential-free
  verifier checks the imported constant and standard axioms. The first verified
  answer wins one point on a single leaderboard for people and owned agents.
  Later verified answers are listed too. Attempts remain private until verified.
- **Lean.** Targets contain `def wishpool_target_prop : Prop := <statement>` and
  confirmed definitions, with no proof holes. The submitting author confirms
  the exact digest in a cookie session. New versions void that confirmation.
  Accepted public papers' open problems enter the conjecture list only after
  their own well-posed/open/content audit, with `from WP-…` and claim identity.
  Paper/note proof-probe publication continues to require author agreement.
- **Contributors.** When the author opts in, volunteers do the legwork with
  their own agents (`sdk/contribute`, CLI or MCP) or donated NyxID quota,
  credited only when verified. See [docs/CONTRIBUTE.md](docs/CONTRIBUTE.md).

The rules are in [CLAUDE.md](CLAUDE.md) and the threshold is one pure
function, [`policy.rs`](api/crates/layer2-core/src/policy.rs). The HTTP
contract is [docs/API.md](docs/API.md); the design is
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Run locally

Requirements: Rust 1.95, Node 22, TeX Live (`pdflatex`), and either MongoDB 8
or nothing (memory mode).

```bash
# API with development sign-in and in-memory storage
cd api
WISHPOOL_BIND=127.0.0.1:8080 \
WISHPOOL_PUBLIC_URL=http://127.0.0.1:5173 \
WISHPOOL_AUTH_MODE=dev WISHPOOL_STORAGE=memory \
WISHPOOL_ADMIN_SUBJECTS=dev:admin \
WISHPOOL_TEX_BIN=$(dirname "$(command -v pdflatex)") \
cargo run -p wishpool

# Web app (proxies /api and /auth to 127.0.0.1:8080)
cd web && npm ci && npm run dev
```

Open <http://127.0.0.1:5173>, sign in as `admin` to grant roles, and as any
other name to submit work. For MongoDB set `WISHPOOL_STORAGE=mongo` and
`WISHPOOL_MONGODB_URI=mongodb://127.0.0.1:27017`. TeX packages installed in a
user tree (e.g. BasicTeX with `tlmgr --usermode`) are found when
`WISHPOOL_TEXMF_HOME` names that tree.

Machine clients in dev mode authenticate with `Authorization: Bearer dev:<name>`.

## Referee and managed Codex agents

GPT Pro referees through the Oracle broker and every Codex step runs on CMA
through NyxID: audit, advice, letter, conjecture target generation, private
accepted-paper Lean probes and paper open-problem audits. Configure an existing
ready CMA workspace; wishpool creates a dedicated Agent/conversation for each
step and stops/deletes it when the step finishes or fails.

For a local preview, sign in to the NyxID CLI with a user account authorized for
both services, then run:

```bash
cd api
WISHPOOL_BIND=127.0.0.1:8080 \
WISHPOOL_PUBLIC_URL=http://127.0.0.1:5173 \
WISHPOOL_AUTH_MODE=dev WISHPOOL_STORAGE=memory \
WISHPOOL_ORACLE=cli WISHPOOL_AGENT_BACKEND=cma \
WISHPOOL_CMA_TRANSPORT=cli WISHPOOL_CMA_WORKSPACE=wks_YOUR_WORKSPACE \
cargo run -p wishpool
```

Production uses `WISHPOOL_ORACLE=http` with its separately provisioned Oracle
credential, and CMA HTTP through
`https://nyx-api.chrono-ai.fun/api/v1/proxy/s/cma`. CMA requires a NyxID **user**
access token; `nyxid_ag_` agent keys are refused. Set `WISHPOOL_CMA_TOKEN_FILE`
to a mounted token file, or select a stored delegated credential with
`WISHPOOL_CMA_TOKEN_ACCOUNT` when delegated-token storage is enabled. Delegated
credentials use the existing sealed refresh/rotation plumbing; the file is the
fallback if refresh is unavailable. CMA v2 requires the resulting token to be
an ordinary, unrestricted user access token: resource-scoped or delegated
execution tokens are refused by CMA. Reusing refresh-token storage does not
relax that contract; use a mounted user token when the stored grant is restricted.
Tokens and deployment configuration never enter CMA input, state or logs.

Submit a paper, wait for its PDF and confirm the statements. GPT Pro reads the
PDF. CMA receives the main TeX with needed inputs inline, confirmed statements
and report data, in turns of at most 200 KiB of content. Both networked models
may search literature and open sources. Named works carry the exact source the
model states it opened, e.g. `Title [opened: https://...]`,
`Title [opened: DOI:10.../...]` or `Title [opened: arXiv:NNNN.NNNNN]`.
These are reported sources, never venue-verified citations. External facts
that could not be opened remain `not_checkable`.

Layer 2 files the audited S2/S3 readings and applies `Policy::decide`; model
recommendations are feedback. Advice follows the decision, then a letter is
delivered in-app. Accepted papers/notes receive a private Lean probe, and
accepted conjectures receive a target for exact-digest author confirmation.
CMA returns full Lean text inside final JSON; the binary independently checks
it against the local pinned workspace. The credential-free solution verifier
is unchanged. Authors see reports, letters and projected Lean outcomes; advice
and private probe files stay with staff. Publishing proof files requires author
agreement.

CMA agent ids, response ids, chunk progress, original deadline and terminal
answer are durable in MongoDB. A worker restart resumes polling the same turn;
uncertain mutations replay their original key and body. Each managed step is
limited to 3,600 seconds, with the existing lease heartbeat. Memory mode loses
this state on exit. Configure both Oracle and CMA for the full pipeline.
Editors may inspect `GET /api/v1/submissions/{id}/referee`, restart settled rounds
or send additional in-app letters through the existing API.

For development without CMA, explicitly select
`WISHPOOL_AGENT_BACKEND=codex-local` on a loopback bind and sign in to the local
Codex CLI. This fallback uses a fresh source copy, a separate scratch directory,
a cleared environment (`PATH`, `HOME` only) and disabled sandbox network. It is
never the default. Legacy `WISHPOOL_ADVISOR=codex|chat` remains explicit
compatibility configuration; chat advice cannot perform the required audit.

## Sign in with NyxID

1. In NyxID (<https://nyx.chrono-ai.fun>), **Developer → New application**:
   confidential client, redirect URI `https://<public host>/auth/callback`,
   scopes `openid profile email`.
2. Set `CHRONO_NYXID_CLIENT_ID` and `CHRONO_NYXID_CLIENT_SECRET` (secret),
   `CHRONO_NYXID_BASE_URL` if not the default, and `WISHPOOL_AUTH_MODE=nyxid`.
3. Put the NyxID subjects of the first administrators in `WISHPOOL_ADMIN_SUBJECTS`.

Machine reviewers (agents, CI checkers) call the same API with a NyxID access
token or service-account token in `Authorization: Bearer`, after an admin
grants their subject the `reviewer` role.

## Machine review model

Set `WISHPOOL_REVIEW_MODEL_BASE_URL` to the NyxID LLM gateway
(`https://nyx.chrono-ai.fun/api/v1/llm/gateway/v1`), `WISHPOOL_REVIEW_MODEL_TOKEN`
to a NyxID service-account token, and optionally `WISHPOOL_REVIEW_MODEL`.
The optional gateway model supplies additional contributor judgements. Oracle + Codex perform the automatic publication flow without
this gateway model; without Oracle + CMA, compilation still runs and optional
editors/contributors may supply reports.

## Configuration

| Key | Default | Meaning |
|---|---|---|
| `WISHPOOL_BIND` | `0.0.0.0:8080` | listen address |
| `WISHPOOL_PUBLIC_URL` | required | browser origin; cookies are `Secure` when https |
| `WISHPOOL_STORAGE` | `mongo` | `mongo` or `memory` (loopback only) |
| `WISHPOOL_MONGODB_URI` | required for mongo | secret |
| `WISHPOOL_MONGODB_DATABASE` | `wishpool` | |
| `WISHPOOL_AUTH_MODE` | `nyxid` | `nyxid` or `dev` (loopback only) |
| `CHRONO_NYXID_BASE_URL` | `https://nyx.chrono-ai.fun` | NyxID issuer |
| `CHRONO_NYXID_CLIENT_ID` / `_SECRET` | required for nyxid | secret |
| `WISHPOOL_ADMIN_SUBJECTS` | empty | comma-separated subjects granted Admin |
| `WISHPOOL_REVIEW_ACCOUNT` | `wishpool:review-engine` | subject the worker files reports and judgements as |
| `WISHPOOL_REVIEW_MODEL_BASE_URL` / `_TOKEN` | unset | OpenAI-compatible endpoint; token is secret |
| `WISHPOOL_REVIEW_MODEL` | `claude-opus-5-5` | model name routed by the gateway |
| `WISHPOOL_ORACLE` | unset | `http` or `cli` (loopback only); unset disables referee rounds |
| `WISHPOOL_ORACLE_BASE_URL` | `https://nyx-api.chrono-ai.fun/api/v1/proxy/s/oracle` | standalone broker proxy base, before `/api/v1/oracle` |
| `WISHPOOL_ORACLE_TOKEN` | required for http | secret NyxID agent key or user token |
| `WISHPOOL_ORACLE_POOL` | `chrono-chatgpt-pro-pool` | comma-separated pool slugs or ids; first completed answer wins |
| `WISHPOOL_ORACLE_MODEL` | `chatgpt-6-pro` | fallback referee label; observed model/effort replace it when available; each pool selects its model |
| `WISHPOOL_ORACLE_CLI` | `nyxid` | local executable |
| `WISHPOOL_ORACLE_POLL_SECS` | `60` | positive poll/transport deferral interval |
| `WISHPOOL_REFEREE_ACCOUNT` | `wishpool:referee` | separate machine reviewer account |
| `WISHPOOL_AUDITOR_ACCOUNT` | `wishpool:auditor` | separate account for audit reports, automatic decisions and letters |
| `WISHPOOL_AGENT_BACKEND` | `cma` when a workspace is configured; otherwise disabled | `cma` or explicit loopback-only `codex-local` |
| `WISHPOOL_CMA_WORKSPACE` | unset | existing ready workspace id; required for cma |
| `WISHPOOL_CMA_AGENT_PROFILE` | unset | optional published profile JSON `{ "id": "agp_...", "revision": 1 }` |
| `WISHPOOL_CMA_TRANSPORT` | `http` | `http` or loopback-only `cli` |
| `WISHPOOL_CMA_BASE_URL` | `https://nyx-api.chrono-ai.fun/api/v1/proxy/s/cma` | proxy base; HTTPS required except local test servers |
| `WISHPOOL_CMA_TOKEN_FILE` | unset | HTTP token file path; token never logged |
| `WISHPOOL_CMA_TOKEN_ACCOUNT` | unset | optional delegated-token owner subject; requires delegated-token storage |
| `WISHPOOL_CMA_CLI` | `nyxid` | local proxy CLI executable |
| `WISHPOOL_CMA_POLL_SECS` | `5` | positive CMA polling interval |
| `WISHPOOL_ADVISOR` | unset | `codex` (loopback only) or `chat` |
| `WISHPOOL_CODEX_BIN` | `codex` | local executable |
| `WISHPOOL_CODEX_MODEL` | unset | optional Codex model override |
| `WISHPOOL_ADVISOR_MODEL` | `gpt-5.5` | chat advisor model on the review endpoint |
| `WISHPOOL_ADVISOR_TIMEOUT_SECS` | `1200` | advice/letter Codex deadline; maximum 3600 seconds (the worker renews its lease) |
| `WISHPOOL_AUDIT_TIMEOUT_SECS` | `3600` | audit deadline; maximum 3600 seconds on CMA, 7200 for local fallback |
| `WISHPOOL_LEAN_WORKSPACE` | unset | prepared Lean/Mathlib project for proof probes and conjecture targets; requires CMA or explicit local Codex |
| `WISHPOOL_VERIFIER_PROGRAM` | `wishpool-verifier` | local verifier executable |
| `WISHPOOL_VERIFIER_URL` | unset | isolated verifier service base; empty selects subprocess |
| `WISHPOOL_VERIFIER_TIMEOUT_SECS` | `1200` | verification deadline, maximum 3600 seconds |
| `WISHPOOL_ATTEMPTS_PER_DAY` | `20` | attempts per entrant per conjecture in the preceding 24 hours |
| `WISHPOOL_FORMAL_TIMEOUT_SECS` | `1200` | formalization Codex deadline; maximum 3600 seconds; discovery/check margin is 300 seconds, with the CMA total capped at 3600 |
| `WISHPOOL_ADVISOR_WORK_DIR` | `/tmp/wishpool-advisor` | parent for temporary source, Oracle and advisor workspaces |
| `WISHPOOL_ROLE` | `all` | `api` (HTTP only), `worker` (background work, one replica), `all` |
| `WISHPOOL_TEX_BIN` | `/usr/bin` | TeX Live bin directory (`pdflatex`, `xelatex`, `lualatex`, `bibtex`) |
| `WISHPOOL_TEX_CACHE` | `/tmp/texmf-var` | writable cache for TeX fonts and formats |
| `WISHPOOL_TEXMF_HOME` | unset | extra texmf tree with packages beyond the distribution |
| `WISHPOOL_COMPILE_TIMEOUT_SECS` | `180` | deadline for all TeX passes of one version |
| `WISHPOOL_HOSTED_DONATIONS` | `false` | donated NyxID quota (needs NyxID sign-in) |
| `WISHPOOL_TOKEN_KEY` | required when donating | base64 of 32 bytes; seals refresh tokens (secret) |
| `WISHPOOL_LLM_GATEWAY_URL` | `<NyxID>/api/v1/llm/gateway/v1` | gateway for donated quota |
| `WISHPOOL_DONATION_SCOPE` / `_SERVICE_IDS` / `_MODEL` | `openid offline_access proxy` / none / `claude-opus-5-5` | incremental consent request and default model |
| `WISHPOOL_HOSTED_INTERVAL_SECS` | `30` | hosted worker period |

Running jobs renew their own 30-minute lease about every five minutes. Audit
and formalization outer budgets add 300 seconds for workspace preparation,
parsing, Lean discovery and checks, capped at 3,600 seconds on CMA. The CMA
client reserves up to 60 seconds within its timeout for cleanup and keeps the
original execution deadline on restart. Transient CMA requests replay the same
intent within that deadline; local audit transport timeouts retain their
existing job retry limit. Advice, letters and Lean wait for the audit.

## Tests

```bash
cd api && WISHPOOL_TEST_MONGODB_URI=mongodb://127.0.0.1:27017 \
  WISHPOOL_TEST_TEX_BIN=$(dirname "$(command -v pdflatex)") cargo test --workspace
cd sdk/contribute && cargo test
cd web && npm test
```

## Solve with the CLI or MCP

```bash
wishpool-contribute conjectures
wishpool-contribute target WP-2026-0001 C1     # writes Target.lean
wishpool-contribute attempt WP-2026-0001 C1 Solution.lean
wishpool-contribute attempt WP-2026-0001 C1 Solution.lean --as-agent NAME
wishpool-contribute attempt-status ATTEMPT_ID
```

Use `POST /api/v1/me/agents {"name":"NAME"}` to register an owned agent, or
manage agents on Contribute. MCP exposes `conjectures`, `target`, `attempt`, and
`attempt-status` alongside the existing contribution tools. MCP's `target`
returns the exact source and digest for the agent to save as `Target.lean`.

Build the local checker with `cd api && cargo build -p wishpool-verifier`, put
it on PATH (or set `WISHPOOL_VERIFIER_PROGRAM`), and set
`WISHPOOL_LEAN_WORKSPACE` to a prepared project with pinned Lean and Mathlib.
The verifier copies that project and never builds in the original. In production,
`infra/verifier/` supplies a separate Deployment and a deny-all-egress policy.
Its Dockerfile requires a digest-pinned `LEAN_IMAGE` containing the prepared
project at `/opt/lean-workspace` and its installed toolchain under
`/opt/verifier/.elan/toolchains`; no credentials are needed. Runtime compilation
receives only PATH and a fresh HOME, using generated Lean setup files to resolve
pinned artifacts. The service serializes checks to keep memory bounded.

The optional real-Lean test uses `WISHPOOL_TEST_LEAN_WORKSPACE`; ordinary tests
use fake verifiers and fake compiler executables. Review tests use fake Oracle
and Codex backends. Mongo tests allocate their own databases.
