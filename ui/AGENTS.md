# teamwire-ui

A web dashboard for teamwire teams. It only reads, unless the server starts
with `TW_UI_ACTIONS=1`: then it can also message members and the lead as the
human user (teamwire ADR 0040), and close one member or a whole team. A Bun server runs the
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

- The app reads, and writes only while actions are on (`TW_UI_ACTIONS=1`
  at start, and `teamwire user-key` worked). Off, every action route answers
  404 and the page shows no action control. The server runs `teamwire` with
  exactly these argument forms and no others: `status --json`, `status <team> --role <role> --json`,
  `teams --json`, `log <team> --json`, `metrics --json`, `metrics <team> --json`,
  `events <team> [--after <seq>] --json`, `events <team> --archive <dir> --json`,
  `decisions <team> --json`, `decisions --grep <text> --json`,
  `decisions --ref=<provider>:<id> --json`,
  `version`, and, only while actions are on,
  `send <team> --from user --to <role|lead> --type <reprompt|note> --text=<text>`,
  `close <team> --role <role> [--stop] [--unverified] --dry-run`, the same
  without `--dry-run`, `close <team> --dry-run`, `close <team>` and
  `user-key`, each with an optional `--home <dir>` first. The `<dir>` after
  `--archive` is an archive directory name that `listArchives` returned,
  never a path.
  - In the write forms a team matches `^[a-z][a-z0-9]{0,11}$` and a role
    `^[a-z][a-z0-9_]{0,15}$`. A text is 1 to 4000 characters with no NUL,
    in one item `--text=<text>`, so a text that starts with `-` cannot read
    as a flag.
  - Time limits: `send` and every `--dry-run` 60 s, a real close 15 minutes,
    every other call 10 s (`status` and decisions searches 30 s).
  - Every action route takes POST only and needs the token cookie, a
    present `Origin` that is ours, `content-type: application/json` and a
    body that its zod schema in `src/shared/api.ts` accepts.
  - A `--grep` text is 1 to 200 characters, never starts with `-`, holds no
    control character, and is one item of the argument array.
  - A ref is valid when its provider matches `^[a-z][a-z0-9_]{0,15}$` and its
    id is 1 to 200 characters with no white space or control character, so
    `github:#3772` is valid. It is one array item, `--ref=<provider>:<id>`,
    so an id that starts with `-` cannot read as a flag.
  - Never `watch`, not even `watch --once`. A watch poll takes the lead's
    watch lease and uses up its announcements.
  - Never `inbox`: it marks messages read.
  - `send` and `close` only in the forms above. Never `launch`, `verify`,
    `resend`, `lead`, `done`, `ack`, `install`, `hook`, `mcp`, `reopen`,
    `handoff` or `metrics export`.
- Start processes with an argument array (`Bun.spawn([...])`), never a shell
  string. Every call has a time limit and is killed when it passes it.
- Remove `TEAMWIRE_TOKEN`, `TEAMWIRE_TOKEN_FILE`, `TEAMWIRE_ROLE`, `TEAMWIRE_TEAM`
  and `TEAMWIRE_USER_KEY` from the environment of every `teamwire` call. With
  the first four set, teamwire treats the caller as a member and hides the
  lead's details. Only `send` gets `TEAMWIRE_USER_KEY` back, set to the key
  the server read at start. The key never appears in a response, a log line
  or an error.
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
