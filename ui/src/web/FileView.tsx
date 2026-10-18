import { useState } from "react";
import type { FileKind, FileText } from "../shared/api.ts";
import { ErrorNote, Loading } from "./components.tsx";
import { useLoad } from "./hooks.ts";
import { Markdown } from "./Markdown.tsx";

/**
 * Shows one file. A Markdown file is rendered from the server's parsed tree,
 * with a switch to its exact text; a log is always plain text. The text is
 * untrusted, so it is never parsed as HTML.
 */
export function FileView({ load, id }: { load: () => Promise<FileText>; id: string }) {
  const { data, error, loading } = useLoad(load, id);
  const [plain, setPlain] = useState(false);
  if (loading) return <Loading what="file" />;
  if (error) return <ErrorNote>{error}</ErrorNote>;
  if (!data) return null;
  return (
    <>
      {data.truncated && <p className="note">Truncated: the file is longer than the server returns.</p>}
      {data.doc && (
        <div className="view-switch" role="group" aria-label="Show the file as">
          <button type="button" className="btn" aria-pressed={!plain} onClick={() => setPlain(false)}>
            Rendered
          </button>
          <button type="button" className="btn" aria-pressed={plain} onClick={() => setPlain(true)}>
            Plain text
          </button>
        </div>
      )}
      {data.doc && !plain ? <Markdown doc={data.doc} /> : <pre className="text">{data.text}</pre>}
    </>
  );
}

export type { FileKind };
