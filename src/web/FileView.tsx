import type { FileKind, FileText } from "../shared/api.ts";
import { ErrorNote, Loading } from "./components.tsx";
import { useLoad } from "./hooks.ts";

/** Shows one file as wrapped plain text. The text is untrusted, so it is never parsed as HTML. */
export function FileView({ load, id }: { load: () => Promise<FileText>; id: string }) {
  const { data, error, loading } = useLoad(load, id);
  if (loading) return <Loading what="file" />;
  if (error) return <ErrorNote>{error}</ErrorNote>;
  if (!data) return null;
  return (
    <>
      {data.truncated && <p className="note">Truncated: the file is longer than the server returns.</p>}
      <pre className="text">{data.text}</pre>
    </>
  );
}

export type { FileKind };
