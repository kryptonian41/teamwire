import index from "../web/index.html";
import { createApp } from "./app.ts";
import { createRunner } from "./cli.ts";
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
  const run = createRunner({ bin: opts.bin, home: opts.home });
  const actions = opts.actions;
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
  console.log(`http://127.0.0.1:${opts.port}/auth?token=${token}`);
}
