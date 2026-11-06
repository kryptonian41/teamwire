import { expect, test } from "bun:test";
import status from "../../../fixtures/status.json";
import teams from "../../../fixtures/teams.json";
import eventsFixture from "../../../fixtures/events.json";
import archivesFixture from "../../../fixtures/archives.json";
import type { ArchiveEntry, FileRef } from "../../shared/api.ts";
import { EventsView, StatusAll, TeamsList, type EventRow } from "../../shared/teamwire.ts";
import { buildDetail, knownRoles, WHY, type DetailSource, type Fact } from "./detail.ts";
import { joinTeam, type TeamModel } from "./teams.ts";

const rows = EventsView.parse(eventsFixture).events;
const payments = joinTeam(StatusAll.parse(status)[0]!, TeamsList.parse(teams).teams);
const archive = (archivesFixture as ArchiveEntry[])[0]!;
const file = (kind: FileRef["kind"], name: string): FileRef => ({ kind, name, bytes: 10, modifiedAt: "2026-10-13T07:00:00.000Z" });
const teamFiles = [file("brief", "api"), file("report", "api"), file("brief", "oldapi"), file("outbox", "oldapi")];
const onTeam = (actions = false, files = teamFiles): DetailSource => ({ page: "team", team: payments, rows, files, actions });
const withTeam = (team: TeamModel): DetailSource => ({ page: "team", team, rows, files: teamFiles, actions: false });
const inArchive: DetailSource = { page: "archive", rows, files: archive.files };
const why = (f: { ok: true } | { ok: false; why: string } | null | undefined) => (f && !f.ok ? f.why : null);
const value = <T,>(f: Fact<T> | null | undefined) => (f && f.ok ? f.value : undefined);

test("a live member takes its facts from status, the roster and the event log", () => {
  const d = buildDetail("api", onTeam(true))!;
  expect(d.subject).toBe("live");
  expect(d.state).toEqual({ word: "working", category: "active" });
  expect(value(d.age)?.label).toBe("20m / 1h");
  expect(value(d.roster)?.title).toBe("Build refund endpoint");
  expect(value(d.kind)).toBe("implementer");
  expect(value(d.branch)).toBe("payments/api");
  expect(value(d.model)).toBe("sonnet · medium");
  expect(d.build).toBe("Started by teamwire 0.4.0");
  expect(d.removal).toBeNull();
  expect(value(d.handoffs)?.map((h) => h.seq)).toEqual([55]);
  expect(value(d.verify)?.map((r) => r.verdict)).toEqual(["FAIL"]);
  expect(d.events.every((r) => r.role === "api")).toBe(true);
  expect(value(d.files)).toEqual([
    { kind: "brief", name: "api", label: "Brief" },
    { kind: "report", name: "api", label: "Report" },
  ]);
  expect(d.resume.ok).toBe(true);
  expect(d.actions).toEqual({ reprompt: true, note: true, close: true });
  expect(d.sessions).toBeNull();
  expect(buildDetail("api", onTeam(false))!.actions).toBeNull();
});

test("a removed member keeps its roster and log facts, and says why the rest is gone", () => {
  const d = buildDetail("oldapi", onTeam(true))!;
  expect(d.subject).toBe("removed");
  expect(d.state.word).toBe("removed");
  expect(why(d.age)).toBe(WHY.removed);
  expect(why(d.branch)).toBe(WHY.removed);
  expect(why(d.resume)).toBe(WHY.removed);
  expect(value(d.roster)?.title).toBe("First refund attempt");
  expect(value(d.kind)).toBe("implementer");
  expect(value(d.generation)).toBe(1);
  expect(value(d.model)).toBe("sonnet · medium");
  expect(d.removal).toBe("oldapi has no passing verify at HEAD 1a2b3c4: it never ran");
  expect(value(d.files)?.map((f) => f.kind)).toEqual(["brief", "outbox"]);
  // No action button for a member that is gone, even while actions are on.
  expect(d.actions).toBeNull();
});

test("an archived member comes from the archive's events and files only", () => {
  const d = buildDetail("ledger", inArchive)!;
  expect(d.subject).toBe("archived");
  expect(d.state.word).toBe("archived");
  for (const f of [d.age, d.roster, d.branch, d.resume]) expect(why(f)).toBe(WHY.archived);
  expect(value(d.kind)).toBe("implementer");
  // ledger's rows carry generation 2 (the reprompt and its second completion).
  expect(value(d.generation)).toBe(2);
  expect(value(d.verify)?.length).toBe(2);
  expect(d.removal).toBeNull();
  expect(buildDetail("oldapi", inArchive)!.removal).toBe("oldapi has no passing verify at HEAD 1a2b3c4: it never ran");
  const impl = buildDetail("impl", { page: "archive", rows: [...rows, { seq: 99, time: rows[0]!.time, type: "member.creating", role: "impl", message: null, data: { kind: "reviewer" } }], files: archive.files })!;
  expect(value(impl.kind)).toBe("reviewer");
  expect(value(impl.files)?.map((f) => f.label)).toEqual(["Brief", "Result", "Report", "Check log"]);
  expect(d.actions).toBeNull();
});

const adopted: EventRow = {
  seq: 100,
  time: "2026-10-13T08:00:00.000Z",
  type: "team.lead_adopted",
  role: null,
  message: null,
  data: { old_session: "11111111-2222-4333-8444-555555555555", new_session: "99999999-2222-4333-8444-555555555555", lead_cwd: "/work/payments" },
};

test("the lead of a live team: liveness, session, adoptions and its own rows", () => {
  const d = buildDetail("lead", { ...onTeam(true), rows: [...rows, adopted] } as DetailSource)!;
  expect(d.subject).toBe("lead");
  expect(d.state).toEqual({ word: "lead working", category: "active" });
  expect(value(d.session)).toBe("11111111-2222-4333-8444-555555555555");
  expect(d.sessions).toEqual([
    { seq: 1, time: rows[0]!.time, event: "created", harness: "claude-code", host: "herdr", oldSession: null, newSession: null, folder: null },
    { seq: 100, time: adopted.time, event: "adopted", harness: null, host: null, oldSession: "11111111-2222-4333-8444-555555555555", newSession: "99999999-2222-4333-8444-555555555555", folder: "/work/payments" },
  ]);
  for (const f of [d.age, d.roster, d.kind, d.generation, d.branch, d.model, d.handoffs, d.verify, d.resume]) expect(why(f)).toBe(WHY.lead);
  expect(why(d.tokens)).toBe(WHY.leadTokens);
  // The lead's rows: team.created, the lead's messages, its deliveries and the adoption.
  expect(d.events.map((r) => r.seq)).toEqual([1, 24, 28, 50, 51, 52, 100]);
  expect(why(d.files)).toBe(WHY.leadFiles);
  const afk = buildDetail("lead", onTeam(false, [file("afk", "decisions"), file("afk", "REPORT"), file("brief", "api")]))!;
  expect(value(afk.files)?.map((f) => [f.kind, f.name])).toEqual([["afk", "decisions"], ["afk", "REPORT"]]);
  expect(d.actions).toBeNull();
});

test("the lead of an archive is archived and has no live session", () => {
  const d = buildDetail("lead", inArchive)!;
  expect(d.state).toEqual({ word: "archived", category: "idle" });
  expect(why(d.session)).toBe(WHY.archived);
  expect(d.sessions?.map((s) => s.event)).toEqual(["created"]);
  expect(why(d.age)).toBe(WHY.lead);
  expect(why(d.files)).toBe(WHY.leadFiles);
});

test("a role no source knows has no record", () => {
  expect(buildDetail("nobody", onTeam())).toBeNull();
  expect(buildDetail("nobody", inArchive)).toBeNull();
  // A live member's role is not an archived one unless the archive's events created it.
  expect(buildDetail("api", { page: "archive", rows: [], files: [] })).toBeNull();
  expect([...knownRoles(onTeam())].sort()).toEqual(["api", "audit", "emails", "fraud", "ledger", "oldapi", "review", "web"]);
  expect(knownRoles(inArchive).has("oldapi")).toBe(true);
});

test("the subtitle: status title for a live member, roster title when status has none, the roster for a removed one", () => {
  const renamed = { ...payments, members: payments.members.map((m) => (m.role === "api" ? { ...m, title: "Status says this" } : m)) };
  expect(buildDetail("api", withTeam(renamed))!.title).toBe("Status says this");
  // A live member the roster does not list keeps its status title.
  expect(buildDetail("api", withTeam({ ...payments, roster: null }))!.title).toBe("Build refund endpoint");
  const untitled = { ...payments, members: payments.members.map((m) => (m.role === "api" ? { ...m, title: "" } : m)) };
  expect(buildDetail("api", withTeam(untitled))!.title).toBe("Build refund endpoint");
  expect(buildDetail("oldapi", onTeam())!.title).toBe("First refund attempt");
  expect(buildDetail("ledger", inArchive)!.title).toBeNull();
  expect(buildDetail("lead", inArchive)!.title).toBe("The team's lead");
});

test("an archive role with files but no events is known, shows its files and marks the event facts missing", () => {
  // The events failed to load or hold no rows; the file list still names impl and review.
  const noEvents: DetailSource = { page: "archive", rows: [], files: archive.files };
  expect([...knownRoles(noEvents)]).toEqual(["impl", "review"]);
  // Event roles come first, then roles that only have files; message and AFK files name no role.
  const withAfk: DetailSource = { page: "archive", rows, files: [...archive.files, file("afk", "decisions")] };
  expect([...knownRoles(withAfk)]).toEqual(["oldapi", "ledger", "web", "emails", "audit", "fraud", "api", "review", "impl"]);
  const d = buildDetail("impl", noEvents)!;
  expect(d.subject).toBe("archived");
  expect(value(d.files)?.map((f) => f.label)).toEqual(["Brief", "Result", "Report", "Check log"]);
  for (const f of [d.kind, d.generation, d.model, d.handoffs, d.verify, d.tokens]) expect(why(f)).toBe(WHY.noEvents);
  for (const f of [d.age, d.roster, d.branch, d.resume]) expect(why(f)).toBe(WHY.archived);
  expect(d.events).toEqual([]);
  expect(d.removal).toBeNull();
  expect(buildDetail("m001", noEvents)).toBeNull();
  expect(buildDetail("decisions", { ...withAfk, rows: [] })).toBeNull();
});
