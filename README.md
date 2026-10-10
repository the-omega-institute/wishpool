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
- **Lean.** Paper/note proof publication needs author approval. After an accepted
  conjecture's letter, the binary elaborates a target with one final `sorry`,
  stores its exact text/digest/toolchain, and asks the author to confirm it in a
  cookie-authenticated browser or request a corrected attempt. New versions void
  confirmation. A target is a statement, not a verified proof. Only an
  author-confirmed target can be attacked; attack attempts belong to Phase B.
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

## Local referee preview

Install and sign in to `nyxid` and `codex` using your own operator accounts. The
NyxID account must be allowed to submit to the Oracle pool. From the repository
root, run the API with development sign-in, memory storage and the local backends:

```bash
cd api
WISHPOOL_BIND=127.0.0.1:8080 \
WISHPOOL_PUBLIC_URL=http://127.0.0.1:5173 \
WISHPOOL_AUTH_MODE=dev WISHPOOL_STORAGE=memory \
WISHPOOL_ADMIN_SUBJECTS=dev:admin \
WISHPOOL_TEX_BIN=$(dirname "$(command -v pdflatex)") \
WISHPOOL_ORACLE=cli WISHPOOL_ADVISOR=codex \
WISHPOOL_ORACLE_POOL=chrono-chatgpt-pro-pool \
WISHPOOL_ORACLE_POLL_SECS=60 \
cargo run -p wishpool
```

Submit a paper, wait for its PDF, and confirm the statements. The referee reads
the PDF through the standalone Oracle broker, reached with
`nyxid proxy request oracle api/v1/oracle/...`; hours of latency are normal.
The durable task is polled between other jobs. Codex audits the report against a
fresh source copy, offline, and Layer 2 files the audited literature and escape
analysis, then applies `Policy::decide` automatically. The model recommendation
is feedback; escape content decides acceptance. Advice follows for every audited
paper, then an English letter is delivered automatically in-app, stating the
decision and record or reasons. Lean runs afterwards, only for accepted work: proof probes for papers/notes and author-confirmed targets for conjectures.
Authors see the checked statement table, full referee report and probe outcomes;
advice and proof-probe files remain staff-only, and publishing proof files needs author agreement. Conjecture targets are shown to their authors for exact-digest confirmation.
Editors remain optional: inspect `GET /api/v1/submissions/{id}/referee`, restart
settled reviews in review with `POST .../referee/restart`, or send an additional
`{subject, body, note?, assessment?}` through `POST .../referee/letters`.
Memory mode loses state on exit; use a dedicated MongoDB database to exercise
recovery across restarts. Chat-only advice cannot perform the Codex audit.

`WISHPOOL_ORACLE_CLI` and `WISHPOOL_CODEX_BIN` select executables;
`WISHPOOL_CODEX_MODEL` is optional. Advisor work directories default to
`/tmp/wishpool-advisor` and the deadline to 1,200 seconds. Child processes receive
only `PATH` and `HOME`; Codex's workspace sandbox has network access disabled.
Both CLI modes are refused on a non-loopback bind. Unset `WISHPOOL_ORACLE`
disables rounds. A configured Oracle with no advisor records failed advisor
steps; configure both for the full preview.

For production use `WISHPOOL_ORACLE=http`, its base URL and a separately
provisioned `WISHPOOL_ORACLE_TOKEN` (NyxID agent key or user token, not a delegated
gateway token). The default broker base is
`https://nyx-api.chrono-ai.fun/api/v1/proxy/s/oracle`.
`WISHPOOL_ADVISOR=chat` uses the existing review model gateway
endpoint/token and `WISHPOOL_ADVISOR_MODEL` (default `gpt-5.5`). Chat advice has
source text and no computation harness; it cannot perform the required Codex
audit. For automatic publication, run the Codex worker on a loopback bind with
the shared durable queue and the HTTP Oracle backend. Recommendations remain
feedback; only the audited escape analysis through `Policy::decide` accepts papers.

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
The optional gateway model supplies additional contributor judgements and
literature leads. Oracle + Codex perform the automatic publication flow without
this gateway model; without Oracle + Codex, compilation still runs and optional
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
| `WISHPOOL_ADVISOR` | unset | `codex` (loopback only) or `chat` |
| `WISHPOOL_CODEX_BIN` | `codex` | local executable |
| `WISHPOOL_CODEX_MODEL` | unset | optional Codex model override |
| `WISHPOOL_ADVISOR_MODEL` | `gpt-5.5` | chat advisor model on the review endpoint |
| `WISHPOOL_ADVISOR_TIMEOUT_SECS` | `1200` | advice/letter Codex deadline; maximum 3600 seconds (the worker renews its lease) |
| `WISHPOOL_AUDIT_TIMEOUT_SECS` | `3600` | Codex audit deadline; maximum 7200 seconds |
| `WISHPOOL_LEAN_WORKSPACE` | unset | prepared Lean/Mathlib project for proof probes and conjecture targets; requires Codex |
| `WISHPOOL_FORMAL_TIMEOUT_SECS` | `1200` | formalization Codex deadline; maximum 3600 seconds, plus 300 seconds for discovery/checks |
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
| `WISHPOOL_OPENALEX_URL` / `_API_KEY` | `https://api.openalex.org` / unset | grounded literature checks; key is secret |

Running jobs renew their own 30-minute lease about every five minutes. Audit
and formalization outer budgets add 300 seconds to the configured timeouts for
workspace preparation, parsing, Lean environment discovery and compiler checks. Audit transport timeouts
retry the same job using the existing attempt limit; advice, letters and Lean
remain pending while the audit retries.

## Tests

```bash
cd api && WISHPOOL_TEST_MONGODB_URI=mongodb://127.0.0.1:27017 \
  WISHPOOL_TEST_TEX_BIN=$(dirname "$(command -v pdflatex)") cargo test --workspace
cd sdk/contribute && cargo test
cd web && npm test
```
