import index from "../web/index.html";
import { createApp } from "./app.ts";
import { createRunner, loadUserKey } from "./cli.ts";
import { parseFlags, type Options } from "./flags.ts";
import { createPoller } from "./poller.ts";
import { createSecurity, makeToken } from "./security.ts";

if (import.meta.main) {
  let opts: Options;
  try {
    opts = parseFlags(Bun.argv.slice(2), process.env);
  } catch (e) {
    console.error(`${(e as Error).message}\nusage: bun run start [--port N] [--interval MS] [--home DIR] [--teamwire PATH]`);
    process.exit(2);
  }
  const token = makeToken();
  // Actions need the user key; without it the dashboard stays read-only and says why.
  let userKey: string | undefined;
  if (opts.actions) {
    const key = await loadUserKey(createRunner({ bin: opts.bin, home: opts.home }));
    if (key.ok) userKey = key.key;
    else console.error(`actions are off: ${key.detail}`);
  }
  const actions = userKey !== undefined;
  const run = createRunner({ bin: opts.bin, home: opts.home, userKey });
  const poller = createPoller({ run, intervalMs: opts.intervalMs, actions });
  const app = createApp({ security: createSecurity({ token, port: opts.port }), poller, run, home: opts.home, actions });
  const handle = (req: Request) => app.handle(req);

  Bun.serve({
    hostname: "127.0.0.1",
    port: opts.port,
    // The event stream is quiet between polls. Keepalives come every 15 s.
    idleTimeout: 60,
    routes: { "/": index, "/auth": handle, "/api/*": handle },
    fetch: () => new Response("not found", { status: 404 }),
  });
  poller.start();
  if (actions) console.log("actions are on: this dashboard can message members and close members and teams");
  console.log(`http://127.0.0.1:${opts.port}/auth?token=${token}`);
}
