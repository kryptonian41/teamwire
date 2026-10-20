# teamwire-ui

A read-only web dashboard for teamwire teams. A Bun server runs the
`teamwire` CLI with `--json`, and a React page shows every team, its
members, their messages and closed teams' archives. This folder sits inside
the teamwire repository, and teamwire's Rust code is in the parent folder.
The CLI reference is `../docs/api/cli.md`. When a teamwire change alters the
`--json` output, change `src/shared/teamwire.ts` in the same commit.

Run every command below from `ui/`. The repository's root `AGENTS.md`
covers the Rust code. Its cargo commands and gates do not cover this folder.

## Layout

| Path | What it holds |
| --- | --- |
| `src/shared/` | The agreed shapes: zod schemas for teamwire's JSON (`teamwire.ts`), the HTTP API (`api.ts`), state colours (`state.ts`). Changed only by the lead |
| `src/server/` | The Bun server: runs the CLI, polls, serves the API and the page |
| `src/web/` | The React page, served by the server through Bun's HTML import |
| `fixtures/` | Invented data in teamwire's real shapes, for tests and the page's mock mode |

## Commands

- `bun install --frozen-lockfile` once per checkout.
- `bun run start`: runs the server and prints the URL to open.
- `bun test src/<dir>`: the tests for one directory.
- `bun run typecheck`: TypeScript over the whole project.
- `TW_UI_LIVE=1 bun test src/shared`: also checks the schemas against the
  real `teamwire` on this machine. Reads only.

## Hard rules

- The app only reads. The server runs `teamwire` with exactly these
  argument forms and no others: `status --json`, `status <team> --role <role> --json`,
  `teams --json`, `log <team> --json`, `metrics --json`, `version`, each
  with an optional `--home <dir>` first.
  - Never `watch`, not even `watch --once`. A watch poll takes the lead's
    watch lease and uses up its announcements.
  - Never `inbox`: it marks messages read.
  - Never `send`, `close`, `launch`, `verify`, `resend`, `lead`, `done`,
    `ack`, `install`, `hook`, `mcp` or `metrics export`.
- Start processes with an argument array (`Bun.spawn([...])`), never a shell
  string. Every call has a time limit and is killed when it passes it.
- Remove `TEAMWIRE_TOKEN`, `TEAMWIRE_TOKEN_FILE`, `TEAMWIRE_ROLE` and `TEAMWIRE_TEAM` from the
  environment of every `teamwire` call. With them set, teamwire treats the
  caller as a member and hides the lead's details.
- Serve files only through `FILE_KINDS` in `src/shared/api.ts`. Never serve
  anything under a team's `members/` directory: it holds member tokens.
- The server binds `127.0.0.1` only. API routes check the `Host` header and
  the token cookie.
- File text is untrusted. The page never parses a string as HTML: no
  `dangerouslySetInnerHTML`, no `innerHTML`. Markdown reaches the page as the
  `MdNode` tree that `src/server/markdown.ts` builds with raw HTML kept as
  text, and `src/web/Markdown.tsx` turns it into React elements. A link is
  made only for an http, https or mailto URL (`safeHref`), and an image is
  never loaded.
- Do not add dependencies or change `bun.lock`. Ask the lead.
- Do not edit `src/shared/`, `fixtures/`, `package.json`, `tsconfig.json`
  or this file unless the lead asks. Ask the lead for a change instead.
- Never delete, skip or loosen a test to make it pass. Fix the code.

## Style

- TypeScript strict. No `any`; parse unknown data with the zod schemas.
- Plain CSS with colour tokens on `:root` and a dark theme under
  `prefers-color-scheme: dark`. No CSS framework.
- Comments explain why, not what.
- Tests go beside the file they test, as `<name>.test.ts`.
