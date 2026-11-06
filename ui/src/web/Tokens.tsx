import { ErrorNote, Loading, RoleLink } from "./components.tsx";
import { useClient, useLoad } from "./hooks.ts";
import { countText, usageTotal } from "./model/memberEvents.ts";
import { tokenTable, type TokenRow } from "./model/tokens.ts";

/** Token totals of the team per role and per model, from `teamwire metrics <team>`. */
export function Tokens({ team, refresh }: { team: string; refresh: string }) {
  const client = useClient();
  const metrics = useLoad(() => client.teamMetrics(team), team, refresh);
  const table = metrics.data ? tokenTable(metrics.data.metrics, team) : null;
  return (
    <section aria-labelledby="tokens-h">
      <h2 id="tokens-h">Tokens</h2>
      {metrics.loading && <Loading what="tokens" />}
      {metrics.error && <ErrorNote>{metrics.error}</ErrorNote>}
      {metrics.data && metrics.data.errors.length > 0 && <ErrorNote>{metrics.data.errors.join("; ")}</ErrorNote>}
      {table && table.byRole.length === 0 && <p className="quiet">No token use recorded for this team.</p>}
      {table && table.byRole.length > 0 && (
        <>
          <TokenTable heading="Role" rows={table.byRole} links />
          <TokenTable heading="Model" rows={table.byModel} />
          <p className="sub">
            All members: {countText(usageTotal(table.total))} tokens. Cache reads are counted again on every request, so they dominate the total.
          </p>
        </>
      )}
    </section>
  );
}

/** `links` makes each row key a role link to its drawer. */
function TokenTable({ heading, rows, links = false }: { heading: string; rows: TokenRow[]; links?: boolean }) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>{heading}</th>
            <th>Input</th>
            <th>Output</th>
            <th>Cache read</th>
            <th>Cache creation</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.key}>
              <td>{links ? <RoleLink role={r.key} /> : r.key}</td>
              <td className="num">{countText(r.usage.input)}</td>
              <td className="num">{countText(r.usage.output)}</td>
              <td className="num">{countText(r.usage.cacheRead)}</td>
              <td className="num">{countText(r.usage.cacheCreation)}</td>
              <td className="num">{countText(usageTotal(r.usage))}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
