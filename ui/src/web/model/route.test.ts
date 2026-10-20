import { expect, test } from "bun:test";
import { href, parseRoute } from "./route.ts";

test("parses every route", () => {
  expect(parseRoute("")).toEqual({ page: "overview" });
  expect(parseRoute("#/")).toEqual({ page: "overview" });
  expect(parseRoute("#/team/payments")).toEqual({ page: "team", team: "payments", member: null });
  expect(parseRoute("#/team/payments/member/api")).toEqual({ page: "team", team: "payments", member: "api" });
  expect(parseRoute("#/history")).toEqual({ page: "history" });
  expect(parseRoute("#/history/ads-2026-10-07-153012")).toEqual({ page: "archive", dir: "ads-2026-10-07-153012" });
  expect(parseRoute("#/history/ads-2026-10-07")).toEqual({ page: "archive", dir: "ads-2026-10-07" });
  expect(parseRoute("#/history/ads-2026-10-07-153012-2")).toEqual({ page: "archive", dir: "ads-2026-10-07-153012-2" });
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
  expect(parseRoute(href.member("payments", "api"))).toEqual({ page: "team", team: "payments", member: "api" });
  expect(parseRoute(href.archive("ads-2026-10-07"))).toEqual({ page: "archive", dir: "ads-2026-10-07" });
  expect(parseRoute(href.overview())).toEqual({ page: "overview" });
});
