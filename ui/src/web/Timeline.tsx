import { useState } from "react";
import type { EventRow } from "../shared/teamwire.ts";
import { ErrorNote, Loading } from "./components.tsx";
import { useNow } from "./hooks.ts";
import { agoText, describeRows, FILTER_LABELS, FILTERS, visibleLines, type Filter } from "./model/timeline.ts";

/** Rows drawn before "Show more": a long run has thousands of routine rows. */
const PAGE = 200;

/**
 * The event rows of a team or one member as a list, newest first, with the
 * filters and the switch for routine rows. `onPickMessage` makes a row's
 * message id a button; leave it out where there is no messages table.
 */
export function Timeline({
  rows,
  loading,
  error,
  onPickMessage,
}: {
  rows: readonly EventRow[];
  loading: boolean;
  error: string | null;
  onPickMessage?: (id: string) => void;
}) {
  const [filter, setFilter] = useState<Filter>("all");
  const [routine, setRoutine] = useState(false);
  const [limit, setLimit] = useState(PAGE);
  const now = useNow(30_000);
  const shown = visibleLines(describeRows(rows), filter, routine);

  return (
    <div className="timeline">
      <div className="filter">
        <div className="chips" role="group" aria-label="Filter events">
          {FILTERS.map((f) => (
            <button
              key={f}
              className="btn"
              aria-pressed={filter === f}
              onClick={() => {
                setFilter(f);
                setLimit(PAGE);
              }}
            >
              {FILTER_LABELS[f]}
            </button>
          ))}
        </div>
        <label className="switch">
          <input type="checkbox" checked={routine} onChange={(e) => setRoutine(e.target.checked)} /> Show routine rows
        </label>
      </div>
      {loading && <Loading what="events" />}
      {error && <ErrorNote>{error}</ErrorNote>}
      {!loading && shown.length === 0 && (
        <p className="quiet">{rows.length === 0 ? "No events recorded yet." : "No events match this filter."}</p>
      )}
      {shown.length > 0 && (
        <ol className="events">
          {shown.slice(0, limit).map((l) => (
            <li key={l.seq} className={l.problem ? "problem" : undefined}>
              <time dateTime={l.time} title={new Date(l.time).toLocaleString()}>
                {agoText(l.time, now)}
              </time>
              <span className="who">{l.role ?? "lead"}</span>
              <span className="what">
                {l.problem && <span className="visually-hidden">Problem: </span>}
                {l.label}
                {l.message && onPickMessage && (
                  <button className="link-btn badge" onClick={() => onPickMessage(l.message!)}>
                    show {l.message}
                  </button>
                )}
                {l.details && <span className="sub details">{l.details}</span>}
              </span>
            </li>
          ))}
        </ol>
      )}
      {shown.length > limit && (
        <button className="btn" onClick={() => setLimit(limit + PAGE)}>
          Show {Math.min(PAGE, shown.length - limit)} more of {shown.length - limit}
        </button>
      )}
    </div>
  );
}
