# wishpool

A venue for mathematicians' own papers. Authors upload LaTeX; the venue reads
every statement, analyses which ones carry new content, publishes the papers
that pass with a public record, and formalizes valuable statements in Lean
with the author's consent.

- **Upload.** A `.tex`, `.zip` or `.tar.gz` source. The venue reads the title,
  authors, abstract and every theorem-like environment and compiles the PDF
  with TeX Live (as arXiv does, honouring `00README.json`).
- **Statements.** The author confirms each statement: kind, main result or
  supporting, dependencies. Misread environments are excluded.
- **Analysis.** S2 literature (is a main result already stated or directly
  implied?) and S3 escape analysis (does the proof carry an escape witness, or
  does the result follow from known ones by binding alone?). Machines and
  volunteer contributors propose; editors decide.
- **Threshold.** A paper is accepted when a main result carries new content
  and is not already known, or settles a named, sourced open problem. Accepted
  papers receive a record `WP-<year>-NNNN` and a public page; the author
  decides whether the analysis is public. Others keep a private report and
  may be revised.
- **Formalization.** Editors propose statements to formalize; the author
  approves each; verified Lean proofs (pinned commit, standard axioms only)
  appear on the public page. The paper's conjectures are followed up and
  results go to the author first.
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
other name to submit a paper. For MongoDB set `WISHPOOL_STORAGE=mongo` and
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
WISHPOOL_ORACLE_POOL=chrono-chatgpt-pro-500-pool \
WISHPOOL_ORACLE_POLL_SECS=60 \
cargo run -p wishpool
```

Submit a paper, wait for its PDF, and confirm the statements. The referee reads
the PDF through the Oracle relay; hours of latency are normal. The durable task
is polled between other jobs. A positive recommendation runs Codex on a source
copy to produce improvement and Lean feasibility advice, then an English letter
draft. Other recommendations skip advice and still draft a letter explaining
the revision needed. An editor can review the rounds through
`GET /api/v1/submissions/{id}/referee`, restart settled rounds with
`POST .../referee/restart`, and send `{subject, body, note?}` in-app with
`POST .../referee/letters`. Authors see only sent letters. Memory mode loses
state on exit; use local MongoDB to exercise recovery across restarts.

`WISHPOOL_ORACLE_CLI` and `WISHPOOL_CODEX_BIN` select executables;
`WISHPOOL_CODEX_MODEL` is optional. Advisor work directories default to
`/tmp/wishpool-advisor` and the deadline to 1,200 seconds. Child processes receive
only `PATH` and `HOME`; Codex's workspace sandbox has network access disabled.
Both CLI modes are refused on a non-loopback bind. Unset `WISHPOOL_ORACLE`
disables rounds. A configured Oracle with no advisor records failed advisor
steps; configure both for the full preview.

For production use `WISHPOOL_ORACLE=http`, its base URL and a separately
provisioned `WISHPOOL_ORACLE_TOKEN` (NyxID agent key or user token, not a delegated
gateway token). `WISHPOOL_ADVISOR=chat` uses the existing review model gateway
endpoint/token and `WISHPOOL_ADVISOR_MODEL` (default `gpt-5.5`). Chat advice has
source text and no computation harness. Review recommendations and drafts do
not accept papers or authorize formalization.

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
Without them, S0 (compilation) still runs and S2/S3 wait for editors and
contributors.

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
| `WISHPOOL_ORACLE_BASE_URL` | `CHRONO_NYXID_BASE_URL` or `https://nyx.chrono-ai.fun` | Oracle HTTP base, before `/api/v1/oracle` |
| `WISHPOOL_ORACLE_TOKEN` | required for http | secret NyxID agent key or user token |
| `WISHPOOL_ORACLE_POOL` | `chrono-chatgpt-pro-500-pool` | pool slug or id |
| `WISHPOOL_ORACLE_MODEL` | `chatgpt-pro` | HTTP model selection and recorded referee label; CLI uses the pool's model |
| `WISHPOOL_ORACLE_CLI` | `nyxid` | local executable |
| `WISHPOOL_ORACLE_POLL_SECS` | `60` | positive poll/transport deferral interval |
| `WISHPOOL_REFEREE_ACCOUNT` | `wishpool:referee` | separate machine reviewer account |
| `WISHPOOL_ADVISOR` | unset | `codex` (loopback only) or `chat` |
| `WISHPOOL_CODEX_BIN` | `codex` | local executable |
| `WISHPOOL_CODEX_MODEL` | unset | optional Codex model override |
| `WISHPOOL_ADVISOR_MODEL` | `gpt-5.5` | chat advisor model on the review endpoint |
| `WISHPOOL_ADVISOR_TIMEOUT_SECS` | `1200` | positive Codex deadline; keep below the 30-minute job lease |
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

## Tests

```bash
cd api && WISHPOOL_TEST_MONGODB_URI=mongodb://127.0.0.1:27017 \
  WISHPOOL_TEST_TEX_BIN=$(dirname "$(command -v pdflatex)") cargo test --workspace
cd sdk/contribute && cargo test
cd web && npm test
```
