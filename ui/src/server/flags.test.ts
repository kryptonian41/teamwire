import { describe, expect, test } from "bun:test";
import { DEFAULT_INTERVAL_MS, DEFAULT_PORT } from "../shared/api.ts";
import { parseFlags } from "./flags.ts";

describe("parseFlags", () => {
  test("defaults", () => {
    expect(parseFlags([], {})).toMatchObject({ port: DEFAULT_PORT, intervalMs: DEFAULT_INTERVAL_MS, bin: "teamwire" });
    expect(parseFlags([], {}).home).toMatch(/\.teamwire$/);
  });
  test("home falls back to TEAMWIRE_HOME, and --home wins", () => {
    expect(parseFlags([], { TEAMWIRE_HOME: "/e" }).home).toBe("/e");
    expect(parseFlags(["--home", "/f"], { TEAMWIRE_HOME: "/e" }).home).toBe("/f");
  });
  test("all flags", () => {
    expect(parseFlags(["--port", "9", "--interval", "2000", "--teamwire", "/b/tw"], {})).toMatchObject({
      port: 9,
      intervalMs: 2000,
      bin: "/b/tw",
    });
  });
  test("actions are on only with TW_UI_ACTIONS=1", () => {
    expect(parseFlags([], {}).actions).toBe(false);
    for (const v of ["0", "true", "yes", " 1", ""]) expect(parseFlags([], { TW_UI_ACTIONS: v }).actions).toBe(false);
    expect(parseFlags([], { TW_UI_ACTIONS: "1" }).actions).toBe(true);
  });
  test("rejects bad values", () => {
    for (const argv of [["--interval", "1999"], ["--port", "0"], ["--port", "x"], ["--port"], ["--host", "0.0.0.0"]]) {
      expect(() => parseFlags(argv, {})).toThrow();
    }
  });
});
