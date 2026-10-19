# teamwire-ui

A read-only web dashboard for [teamwire](..) agent teams.

```bash
bun install --frozen-lockfile
bun run start            # prints http://127.0.0.1:7878/auth?token=...
```

Open the printed URL. The token in it sets a cookie, so the API answers only
your browser. The page shows every team under `$TEAMWIRE_HOME`, refreshed
every 5 seconds, and the archives of closed teams.

The server never changes teamwire state. To act on a team, use the lead's
Claude Code session.
