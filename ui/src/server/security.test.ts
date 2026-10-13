import { describe, expect, test } from "bun:test";
import { createSecurity, makeToken } from "./security.ts";

const token = "ab".repeat(32);
const sec = createSecurity({ token, port: 7878 });
const req = (path: string, headers: Record<string, string> = {}) =>
  new Request(`http://127.0.0.1:7878${path}`, { headers: { host: "127.0.0.1:7878", ...headers } });
const cookie = { cookie: `tw_ui_7878=${token}` };

describe("makeToken", () => {
  test("is 64 hex characters and differs per call", () => {
    expect(makeToken()).toMatch(/^[0-9a-f]{64}$/);
    expect(makeToken()).not.toBe(makeToken());
  });
});

describe("checkApi", () => {
  test("accepts the cookie with either host", () => {
    expect(sec.checkApi(req("/api/snapshot", cookie))).toBeNull();
    expect(sec.checkApi(req("/api/snapshot", { ...cookie, host: "localhost:7878" }))).toBeNull();
  });
  test("accepts an own Origin and no Origin", () => {
    expect(sec.checkApi(req("/api/x", { ...cookie, origin: "http://127.0.0.1:7878" }))).toBeNull();
    expect(sec.checkApi(req("/api/x", { ...cookie, origin: "http://localhost:7878" }))).toBeNull();
  });
  test("accepts the cookie among others", () => {
    expect(sec.checkApi(req("/api/x", { cookie: `a=b; tw_ui_7878=${token}; c=d` }))).toBeNull();
  });

  const refuse = async (headers: Record<string, string>) => {
    const res = sec.checkApi(req("/api/x", headers));
    expect(res?.status).toBe(403);
    expect(await res?.json()).toMatchObject({ code: "forbidden" });
  };
  test("refuses a missing cookie", () => refuse({}));
  test("refuses a wrong cookie", () => refuse({ cookie: "tw_ui_7878=nope" }));
  test("refuses a cookie named for another port", () => refuse({ cookie: `tw_ui_7879=${token}` }));
  test("refuses a cookie of another name", () => refuse({ cookie: `other=${token}` }));
  test("refuses a wrong host", () => refuse({ ...cookie, host: "evil.example:7878" }));
  test("refuses the right host with the wrong port", () => refuse({ ...cookie, host: "127.0.0.1:1" }));
  test("refuses a missing host", () => {
    const r = new Request("http://127.0.0.1:7878/api/x", { headers: cookie });
    r.headers.delete("host");
    expect(sec.checkApi(r)?.status).toBe(403);
  });
  test("refuses a foreign Origin", () => refuse({ ...cookie, origin: "https://evil.example" }));
  test("refuses Origin null", () => refuse({ ...cookie, origin: "null" }));
  test("refuses an https Origin", () => refuse({ ...cookie, origin: "https://127.0.0.1:7878" }));
});

describe("auth", () => {
  test("a matching token sets the cookie and redirects with 303", () => {
    const res = sec.auth(req(`/auth?token=${token}`));
    expect(res.status).toBe(303);
    expect(res.headers.get("location")).toBe("/");
    expect(res.headers.get("set-cookie")).toBe(`tw_ui_7878=${token}; HttpOnly; SameSite=Strict; Path=/`);
  });
  test("a wrong, short or missing token is 403 with no cookie", () => {
    for (const path of ["/auth?token=nope", `/auth?token=${token}00`, "/auth", "/auth?token="]) {
      const res = sec.auth(req(path));
      expect(res.status).toBe(403);
      expect(res.headers.get("set-cookie")).toBeNull();
    }
  });
  test("a wrong host is 403 even with the right token", () => {
    expect(sec.auth(req(`/auth?token=${token}`, { host: "evil.example" })).status).toBe(403);
  });
});
