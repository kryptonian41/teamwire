import { expect, test } from "bun:test";
import { href, parseRoute, roleHref, TABS, type RolePlace } from "./route.ts";

test("parses every route", () => {
  expect(parseRoute("")).toEqual({ page: "overview" });
  expect(parseRoute("#/")).toEqual({ page: "overview" });
  expect(parseRoute("#/team/payments")).toEqual({ page: "team", team: "payments", tab: "overview", member: null });
  expect(parseRoute("#/team/payments/member/api")).toEqual({ page: "team", team: "payments", tab: "overview", member: "api" });
  expect(parseRoute("#/history")).toEqual({ page: "history" });
  expect(parseRoute("#/history/ads-2026-10-07-153012")).toEqual({ page: "archive", dir: "ads-2026-10-07-153012", member: null });
  expect(parseRoute("#/history/ads-2026-10-07")).toEqual({ page: "archive", dir: "ads-2026-10-07", member: null });
  expect(parseRoute("#/history/ads-2026-10-07-153012-2")).toEqual({ page: "archive", dir: "ads-2026-10-07-153012-2", member: null });
});

test("rejects names that could escape an API path", () => {
  expect(parseRoute("#/team/..").page).toBe("notfound");
  expect(parseRoute("#/team/a%2Fb").page).toBe("notfound");
  expect(parseRoute("#/team/Payments").page).toBe("notfound");
  expect(parseRoute("#/team/x/member/../y").page).toBe("notfound");
  expect(parseRoute("#/history/../etc").page).toBe("notfound");
  expect(parseRoute("#/nope").page).toBe("notfound");
});

test("href builders round-trip through the parser", () => {
  expect(parseRoute(href.member("payments", "api"))).toEqual({ page: "team", team: "payments", tab: "overview", member: "api" });
  expect(parseRoute(href.team("payments"))).toEqual({ page: "team", team: "payments", tab: "overview", member: null });
  expect(parseRoute(href.team("payments", "tokens"))).toEqual({ page: "team", team: "payments", tab: "tokens", member: null });
  expect(parseRoute(href.member("payments", "api", "timeline"))).toEqual({ page: "team", team: "payments", tab: "timeline", member: "api" });
  expect(parseRoute(href.archive("ads-2026-10-07"))).toEqual({ page: "archive", dir: "ads-2026-10-07", member: null });
  expect(parseRoute(href.overview())).toEqual({ page: "overview" });
});

test("each of the five tabs has its own route, with or without a member", () => {
  for (const tab of TABS) {
    expect(parseRoute(`#/team/payments/${tab}`)).toEqual({ page: "team", team: "payments", tab, member: null });
    expect(parseRoute(`#/team/payments/${tab}/member/api`)).toEqual({ page: "team", team: "payments", tab, member: "api" });
  }
  expect(TABS).toEqual(["overview", "messages", "decisions", "tokens", "timeline"]);
});

test("an unknown tab, a bad member under a tab, or extra parts are not found", () => {
  expect(parseRoute("#/team/payments/flow").page).toBe("notfound");
  expect(parseRoute("#/team/payments/Messages").page).toBe("notfound");
  expect(parseRoute("#/team/payments/messages/member/..").page).toBe("notfound");
  expect(parseRoute("#/team/payments/messages/api").page).toBe("notfound");
  expect(parseRoute("#/team/payments/messages/member/api/x").page).toBe("notfound");
  expect(parseRoute("#/team/payments/member").page).toBe("notfound");
});

test("an archive member, the archive's lead and the team's lead each have a route", () => {
  expect(parseRoute("#/history/ads-2026-10-07-153012/member/impl")).toEqual({ page: "archive", dir: "ads-2026-10-07-153012", member: "impl" });
  expect(parseRoute("#/history/ads-2026-10-07/member/lead")).toEqual({ page: "archive", dir: "ads-2026-10-07", member: "lead" });
  expect(parseRoute("#/team/payments/member/lead")).toEqual({ page: "team", team: "payments", tab: "overview", member: "lead" });
  expect(parseRoute("#/team/payments/timeline/member/lead")).toEqual({ page: "team", team: "payments", tab: "timeline", member: "lead" });
  expect(parseRoute(href.archiveMember("ads-2026-10-07", "review"))).toEqual({ page: "archive", dir: "ads-2026-10-07", member: "review" });
});

test("a crafted archive or role name in an archive member route is refused", () => {
  expect(parseRoute("#/history/../etc/member/impl").page).toBe("notfound");
  expect(parseRoute("#/history/ads/member/impl").page).toBe("notfound");
  expect(parseRoute("#/history/ads-2026-10-07/member/..").page).toBe("notfound");
  expect(parseRoute("#/history/ads-2026-10-07/member/Impl").page).toBe("notfound");
  expect(parseRoute("#/history/ads-2026-10-07/member/a%2Fb").page).toBe("notfound");
  expect(parseRoute("#/history/ads-2026-10-07/member").page).toBe("notfound");
  expect(parseRoute("#/history/ads-2026-10-07/member/impl/x").page).toBe("notfound");
  expect(parseRoute("#/history/ads-2026-10-07/impl").page).toBe("notfound");
});

test("a role link opens the drawer only for a known role or the lead", () => {
  const team: RolePlace = { page: "team", team: "payments", tab: "timeline", roles: new Set(["api", "oldapi"]) };
  expect(roleHref("api", team)).toBe("#/team/payments/timeline/member/api");
  expect(roleHref("oldapi", team)).toBe("#/team/payments/timeline/member/oldapi");
  expect(roleHref("lead", team)).toBe("#/team/payments/timeline/member/lead");
  expect(roleHref("user", team)).toBeNull();
  expect(roleHref("api@billing", team)).toBeNull();
  expect(roleHref("lead@billing", team)).toBeNull();
  expect(roleHref("nobody", team)).toBeNull();
  const archive: RolePlace = { page: "archive", dir: "ads-2026-10-07", roles: new Set(["impl", "user", "x@y"]) };
  expect(roleHref("impl", archive)).toBe("#/history/ads-2026-10-07/member/impl");
  expect(roleHref("lead", archive)).toBe("#/history/ads-2026-10-07/member/lead");
  // Never linked, even when a source lists them.
  expect(roleHref("user", archive)).toBeNull();
  expect(roleHref("x@y", archive)).toBeNull();
  expect(roleHref("api", archive)).toBeNull();
});
