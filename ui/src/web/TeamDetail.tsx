import { useEffect, useRef, useState } from "react";
import type { Snapshot } from "../shared/api.ts";
import { AfkFiles, Empty, RefChips, RoleLink, RolePlaceContext, StateChip } from "./components.tsx";
import { Drawer } from "./Drawer.tsx";
import { CloseMemberDialog, CloseTeamDialog, MessageButton } from "./Actions.tsx";
import { REFRESH_MS, useClient, useLoad, useNow, useTimeline } from "./hooks.ts";
import { MemberTable } from "./MemberTable.tsx";
import { MessageSection } from "./Messages.tsx";
import { NeedsLook } from "./NeedsLook.tsx";
import { href, TABS, type Tab } from "./model/route.ts";
import { afkLine, refText, watchLine } from "./model/status.ts";
import { livenessInfo, refreshKey, type TeamModel } from "./model/teams.ts";
import { removalText } from "./model/memberEvents.ts";
import { buildDetail, knownRoles, type DetailSource } from "./model/detail.ts";
import { Timeline } from "./Timeline.tsx";
import { Decisions } from "./Decisions.tsx";
import { Tokens } from "./Tokens.tsx";
import type { DecisionQuery } from "../shared/api.ts";

const TAB_LABELS: Record<Tab, string> = { overview: "Overview", messages: "Messages", decisions: "Decisions", tokens: "Tokens", timeline: "Timeline" };

export function TeamDetail({
  team,
  tab,
  member,
  snapshot,
}: {
  team: TeamModel | undefined;
  tab: Tab;
  member: string | null;
  snapshot: Snapshot | null;
}) {
  const client = useClient();
  const name = team?.name ?? "";
  const tick = useNow(REFRESH_MS);
  const refresh = team ? `${refreshKey(team.status)}|${tick}` : "";
  const files = useLoad(() => client.teamFiles(name), name, refresh);
  const timeline = useTimeline((after) => client.teamTimeline(name, after), name, refresh);
  const [pick, setPick] = useState<{ id: string; n: number } | null>(null);
  // The decision search lives above the tabs, so it is kept across tab changes.
  const [query, setQuery] = useState<DecisionQuery | null>(null);
  // Kept here, not in the drawer, so the result stays on screen after the member is gone.
  const [closing, setClosing] = useState<{ team: string; role: string | null } | null>(null);
  const closeDialog =
    closing &&
    (closing.role === null ? (
      <CloseTeamDialog team={closing.team} onClose={() => setClosing(null)} />
    ) : (
      <CloseMemberDialog team={closing.team} role={closing.role} onClose={() => setClosing(null)} />
    ));
  const head = useRef<HTMLDivElement>(null);
  // Focus scrolls the page only until the element is in the viewport, where the sticky header would cover it.
  useEffect(() => {
    const el = head.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      // Rounded up plus a hair, so a sub-pixel header height never leaves a focused edge under it.
      document.documentElement.style.scrollPaddingTop = `${Math.ceil(el.getBoundingClientRect().height) + 2}px`;
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      document.documentElement.style.scrollPaddingTop = "";
    };
  }, [name, snapshot !== null]);
  if (!snapshot) return <Empty title="Waiting for the first poll…" />;
  if (!team || team.name === null) {
    return (
      <>
        <p className="crumbs">
          <a href={href.overview()}>← Teams</a>
        </p>
        <Empty title="No team with that name">
          It may have closed. Closed teams are under <a href={href.history()}>History</a>.
        </Empty>
        {closeDialog}
      </>
    );
  }
  const s = team.status;
  const live = livenessInfo(s.lead);
  const source: DetailSource = { page: "team", team, rows: timeline.rows, files: files.data ?? [], actions: snapshot.actions };

  return (
    <RolePlaceContext.Provider value={{ page: "team", team: name, tab, roles: knownRoles(source) }}>
      <div className="team-head" ref={head}>
        <p className="crumbs">
          <a href={href.overview()}>← Teams</a>
        </p>
        <div className="team-title">
          <h1>{team.title}</h1>
          {/* Empty while actions are off, and then it takes no space. */}
          <div className="team-actions">
            {snapshot.actions && (
              <>
                <MessageButton team={name} to="lead" type="note" label="Message lead" title={`Note to the lead of ${name}`} />
                <button className="btn" onClick={() => setClosing({ team: name, role: null })}>
                  Close team
                </button>
              </>
            )}
          </div>
        </div>
        <div className="meta">
          <code>{team.name}</code>
          <StateChip word={live.label} category={live.category} />
          <span className="chip">{s.state ?? "state unknown"}</span>
          <span className="chip">{watchLine(s.watch)}</span>
          {s.afk && (s.afk.spent ? <StateChip word={afkLine(s.afk)} category="attention" /> : <span>{afkLine(s.afk)}</span>)}
          <RefChips
            refs={s.refs}
            onPick={(r) => {
              setQuery({ ref: refText(r) });
              location.hash = href.team(name, "decisions");
            }}
          />
        </div>
        <nav className="tabnav" aria-label="Team sections">
          {TABS.map((t) => (
            <a key={t} href={href.team(name, t)} aria-current={t === tab ? "page" : undefined}>
              {TAB_LABELS[t]}
            </a>
          ))}
        </nav>
      </div>
      {s.errors.length > 0 && (
        <ul className="errors page-errors">
          {s.errors.map((e, i) => (
            <li key={i}>{e}</li>
          ))}
        </ul>
      )}

      {tab === "overview" && (
        <>
          {s.afk && <AfkFiles id={name} files={files.data ?? []} load={(n) => client.teamFile(name, "afk", n)} />}
          <h2>Members</h2>
          <MemberTable team={team} tab={tab} rows={timeline.rows} />
          <NeedsLook team={team} actions={snapshot.actions} />
          {team.removed.length > 0 && (
            <details>
              <summary>Removed members ({team.removed.length})</summary>
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>Role</th>
                      <th>Title</th>
                      <th>Kind</th>
                      <th>Generation</th>
                      <th>Why removed</th>
                    </tr>
                  </thead>
                  <tbody>
                    {team.removed.map((m) => (
                      <tr key={m.role}>
                        <td>
                          <RoleLink role={m.role}>
                            <code>{m.role}</code>
                          </RoleLink>
                        </td>
                        <td>{m.title}</td>
                        <td>{m.kind}</td>
                        <td>{m.generation}</td>
                        <td>{removalText(timeline.rows, m.role)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </details>
          )}
        </>
      )}

      {tab === "messages" && <MessageSection team={team} pick={pick} />}

      {tab === "decisions" && <Decisions team={name} refresh={refresh} query={query} onQuery={setQuery} />}

      {tab === "tokens" && <Tokens team={name} refresh={refresh} />}

      {tab === "timeline" && (
        <>
          <h2>Timeline</h2>
          <Timeline
            rows={timeline.rows}
            loading={timeline.loading}
            error={timeline.error}
            onPickMessage={(id) => {
              setPick({ id, n: (pick?.n ?? 0) + 1 });
              location.hash = href.team(name, "messages");
            }}
          />
        </>
      )}

      {member !== null && (
        <Drawer
          role={member}
          detail={buildDetail(member, source)}
          loading={timeline.loading}
          error={timeline.error}
          scope={name}
          team={name}
          loadFile={(f) => client.teamFile(name, f.kind, f.name)}
          onCloseMember={() => setClosing({ team: name, role: member })}
          onClose={() => {
            location.hash = href.team(name, tab);
          }}
        />
      )}
      {closeDialog}
    </RolePlaceContext.Provider>
  );
}
