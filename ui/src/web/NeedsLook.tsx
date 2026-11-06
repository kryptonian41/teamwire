import { DeliveryChip } from "./components.tsx";
import { MessageButton } from "./Actions.tsx";
import { replyPrefix, replyTarget } from "./model/actions.ts";
import { questionRows, startedByLine, unreadLines } from "./model/status.ts";
import type { TeamModel } from "./model/teams.ts";

/**
 * The team's own "Needs a look" section, from `status --json`: unread deliveries, every unanswered
 * question, older builds, stuck deliveries and undelivered results. Not the cross-team queue.
 */
export function NeedsLook({ team, actions }: { team: TeamModel; actions: boolean }) {
  const s = team.status;
  const unread = unreadLines(s);
  const questions = questionRows(s);
  const older = team.members.filter((m) => m.older_build);
  if (s.stuck_deliveries.length === 0 && s.undelivered_results.length === 0 && unread.length === 0 && questions.length === 0 && older.length === 0) return null;
  return (
    <>
      <h2>Needs a look</h2>
      {(unread.length > 0 || questions.length > 0) && (
        <>
          <h3>Waiting on a reply</h3>
          {unread.length > 0 && (
            <ul className="results">
              {unread.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          )}
          {questions.length > 0 && (
            <ul className="questions">
              {questions.map((q) => {
                // Plan D11: a reply goes to an asker who is an implementer or investigator of this team.
                const to = actions && team.name !== null ? replyTarget(q.from, team.members) : null;
                return (
                  <li key={q.id} className="question">
                    <code>{q.id}</code>
                    <span className="sub">
                      {q.from} → {q.to} · asked {q.age} ago
                    </span>
                    <span className="qtext">{q.text ?? "question text not in the latest messages"}</span>
                    {/* Empty while no reply is offered, and then it takes no space. The question stays unanswered. */}
                    <span className="question-actions">
                      {to !== null && team.name !== null && (
                        <MessageButton team={team.name} to={to} type="note" label="Reply with a note" title={`Reply to ${q.id} with a note to ${to}`} prefix={replyPrefix(q.id)} />
                      )}
                    </span>
                  </li>
                );
              })}
            </ul>
          )}
        </>
      )}
      {older.length > 0 && (
        <>
          <h3>Older builds</h3>
          <ul className="results">
            {older.map((m) => (
              <li key={m.role}>
                <strong>{m.role}</strong>: {startedByLine(m)}
              </li>
            ))}
          </ul>
        </>
      )}
      {s.stuck_deliveries.length > 0 && (
        <>
          <h3>Stuck deliveries</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Message</th>
                  <th>To</th>
                  <th>Delivery</th>
                  <th className="num">Waiting</th>
                </tr>
              </thead>
              <tbody>
                {s.stuck_deliveries.map((d) => (
                  <tr key={`${d.display_id}>${d.to}`}>
                    <td>
                      <code>{d.display_id}</code>
                    </td>
                    <td>{d.to}</td>
                    <td>
                      <DeliveryChip to={d.to} state={d.state} />
                    </td>
                    <td className="num">{d.age_s} s</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
      {s.undelivered_results.length > 0 && (
        <>
          <h3>Undelivered results</h3>
          <ul className="results">
            {s.undelivered_results.map((r) => (
              <li key={`${r.role}-${r.n}`}>
                Result {r.n} from <strong>{r.role}</strong>: {r.report_state} <code>{r.path}</code>
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  );
}
