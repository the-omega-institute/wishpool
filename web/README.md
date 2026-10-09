# wishpool web

The browser front end for wishpool: React 19, Vite 7, TypeScript (strict). It
talks only to the public HTTP API described in [`../docs/API.md`](../docs/API.md).

## Scripts

| Command                | What it does                               |
| ---------------------- | ------------------------------------------ |
| `npm ci`               | Install the locked dependencies            |
| `npm run dev`          | Vite dev server on http://localhost:5173   |
| `npm run build`        | Production bundle in `dist/`               |
| `npm run typecheck`    | `tsc --noEmit`                             |
| `npm run lint`         | ESLint, zero warnings allowed              |
| `npm run format:check` | Prettier check (`npm run format` rewrites) |
| `npm run test`         | Vitest + Testing Library (jsdom)           |

## Dev proxy

`npm run dev` forwards `/api` and `/auth` to the API binary at
`http://127.0.0.1:8080`, so the `wp_session` cookie stays same-origin. Start
the API first; sign-in goes through `/auth/login?return_to=…`.

## Pages

| Path                    | Who                | What                                                             |
| ----------------------- | ------------------ | ---------------------------------------------------------------- |
| `/`                     | anyone             | What the venue does; recently accepted papers                    |
| `/papers`               | anyone             | Accepted papers                                                  |
| `/papers/{record}`      | anyone             | Public page: statements, PDF, Lean badges, analysis if public    |
| `/policy`               | anyone             | Stages S0–S3 and the threshold                                   |
| `/submit`               | signed in          | Upload LaTeX source with the AI disclosure                       |
| `/submissions`          | signed in          | My papers                                                        |
| `/submissions/{id}`     | authors, staff     | Workspace: confirm statements, stages, decision, analysis, tools |
| `/queue`                | editors, reviewers | Review queue                                                     |
| `/people`               | admins             | Roles                                                            |
| `/contribute`           | anyone             | Donating tokens, CLI/MCP setup, credit rules                     |
| `/tasks`, `/tasks/{id}` | signed in          | Contributor tasks; lease, submit                                 |
| `/contributors`         | anyone             | Credit per contributor                                           |

## Layout

- `src/api` — wire types (`types.ts`, copied from `docs/API.md`), the typed
  `fetch` client with `ApiError` problem parsing and multipart uploads, and
  data hooks.
- `src/lib` — pure logic with unit tests: LaTeX statements to Markdown with
  KaTeX math (`latex.ts`), upload checks and metadata (`upload.ts`), the
  statement confirmation body (`confirm.ts`), editor report, judgement,
  verification and conjecture bodies (`editorForms.ts`), contribution bodies
  per task kind (`tasks.ts`), current vs superseded reports (`reports.ts`),
  labels, links, the donation meter, exact escape-rate fractions.
- `src/components`, `src/pages` — UI; `src/components/workspace` holds the
  author and editor panels of the paper workspace. `src/styles` — tokens and
  one stylesheet (light and dark via `prefers-color-scheme`).
- `src/routing` — a small History-API router. Production serving must fall
  back to `index.html` for unknown paths (nginx: `try_files $uri $uri/ /index.html;`).
