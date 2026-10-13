// Host, Origin and token checks. The server binds 127.0.0.1 already; these
// checks stop another web page (DNS rebinding, cross-site requests) and
// another local user from reading team data through the browser.

import { timingSafeEqual } from "node:crypto";
import { TOKEN_COOKIE, type Problem } from "../shared/api.ts";

export function makeToken(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export function json(body: unknown, status = 200, headers: HeadersInit = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store", ...headers },
  });
}

export function problem(status: number, p: Problem): Response {
  return json(p, status);
}

/** Compares in constant time for equal lengths; a length mismatch is a plain no. */
function sameToken(given: string, expected: string): boolean {
  const a = Buffer.from(given);
  const b = Buffer.from(expected);
  if (a.length !== b.length) return false;
  return timingSafeEqual(a, b);
}

function cookieValue(header: string | null, name: string): string | null {
  if (!header) return null;
  for (const part of header.split(";")) {
    const eq = part.indexOf("=");
    if (eq < 0) continue;
    if (part.slice(0, eq).trim() === name) return part.slice(eq + 1).trim();
  }
  return null;
}

export interface Security {
  /** Null when the request may use the API, else the 403 response. */
  checkApi(req: Request): Response | null;
  /** `GET /auth?token=T`. */
  auth(req: Request): Response;
}

export function createSecurity(opts: { token: string; port: number }): Security {
  const hosts = new Set([`127.0.0.1:${opts.port}`, `localhost:${opts.port}`]);
  const origins = new Set([`http://127.0.0.1:${opts.port}`, `http://localhost:${opts.port}`]);
  const forbidden = (detail: string) => problem(403, { code: "forbidden", detail });

  function checkHostAndOrigin(req: Request): Response | null {
    const host = req.headers.get("host");
    if (host === null || !hosts.has(host)) return forbidden("unexpected Host header");
    const origin = req.headers.get("origin");
    if (origin !== null && !origins.has(origin)) return forbidden("unexpected Origin header");
    return null;
  }

  return {
    checkApi(req) {
      const bad = checkHostAndOrigin(req);
      if (bad) return bad;
      const cookie = cookieValue(req.headers.get("cookie"), TOKEN_COOKIE);
      if (cookie === null || !sameToken(cookie, opts.token)) return forbidden("missing or wrong token cookie");
      return null;
    },
    auth(req) {
      const bad = checkHostAndOrigin(req);
      if (bad) return bad;
      const given = new URL(req.url).searchParams.get("token");
      if (given === null || !sameToken(given, opts.token)) return forbidden("wrong token");
      return new Response(null, {
        status: 303,
        headers: {
          location: "/",
          "set-cookie": `${TOKEN_COOKIE}=${opts.token}; HttpOnly; SameSite=Strict; Path=/`,
        },
      });
    },
  };
}
