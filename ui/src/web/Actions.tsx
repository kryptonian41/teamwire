// The write actions: messages as the user, and closing a member or the team.
// Each one asks for a confirm first and shows the CLI's result plainly. The
// page never retries an action by itself.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { MAX_TEXT_CHARS, type ActionResult } from "../shared/api.ts";
import { ApiError } from "./api.ts";
import { useClient } from "./hooks.ts";
import { canClose, canCloseTeam, closeOutcome, closeRisks, leadNoteLines, sendOutcome, type CloseFlag, type CloseFlags, type Outcome } from "./model/actions.ts";

/**
 * A modal `<dialog>`: the browser keeps focus inside it and the page behind
 * it inert. It is portalled to `body`, because the page root is inert while
 * the member drawer is open, and a dialog inside it could not be used.
 */
export function ActionDialog({ title, busy, onClose, children }: { title: string; busy: boolean; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  const id = useId();
  useEffect(() => {
    const d = ref.current;
    d?.showModal();
    return () => d?.close();
  }, []);
  return createPortal(
    <dialog
      ref={ref}
      className="action-dialog"
      aria-labelledby={id}
      onCancel={(e) => {
        // Escape closes only between steps: a running command keeps its dialog until it answers.
        e.preventDefault();
        if (!busy) onClose();
      }}
      // The drawer closes on any Escape that reaches window; this one belongs to the dialog.
      onKeyDown={(e) => e.key === "Escape" && e.stopPropagation()}
    >
      <h2 id={id}>{title}</h2>
      {children}
    </dialog>,
    document.body,
  );
}

export function errorOutcome(e: unknown): Outcome {
  return { ok: false, text: e instanceof ApiError ? `the server refused the request: ${e.problem.detail}` : `the request failed: ${(e as Error).message}` };
}

export function OutcomeLine({ outcome }: { outcome: Outcome }) {
  return (
    <p className={outcome.ok ? "outcome" : "outcome note error"} role="status">
      {outcome.text}
    </p>
  );
}

const TYPE_HINT: Record<"reprompt" | "note", string> = {
  reprompt: "A reprompt starts the member's next generation with this text.",
  note: "A note is typed into the recipient's session as information.",
};

/** A button that opens a message dialog. `to` is a role or `lead`. */
export function MessageButton(props: { team: string; to: string; type: "reprompt" | "note"; label: string; title: string; prefix?: string }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button className="btn" onClick={() => setOpen(true)}>
        {props.label}
      </button>
      {open && <MessageDialog {...props} onClose={() => setOpen(false)} />}
    </>
  );
}

function MessageDialog({ team, to, type, title, prefix = "", onClose }: { team: string; to: string; type: "reprompt" | "note"; title: string; prefix?: string; onClose: () => void }) {
  const client = useClient();
  const [text, setText] = useState(prefix);
  const [step, setStep] = useState<"write" | "confirm" | "busy" | "done">("write");
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const chars = [...text].length;
  const valid = text.trim() !== "" && text.trim() !== prefix.trim() && chars <= MAX_TEXT_CHARS && !text.includes("\0");

  const send = async () => {
    setStep("busy");
    try {
      setOutcome(sendOutcome(await client.send(team, { to, type, text }), team));
    } catch (e) {
      setOutcome(errorOutcome(e));
    }
    setStep("done");
  };

  return (
    <ActionDialog title={title} busy={step === "busy"} onClose={onClose}>
      {step === "write" && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            if (valid) setStep("confirm");
          }}
        >
          <p className="sub">{TYPE_HINT[type]} It is sent from <code>user</code>, the human user{to === "lead" ? "." : ", and the lead gets a copy."}</p>
          <label className="field">
            Text
            <textarea value={text} rows={6} autoFocus onChange={(e) => setText(e.target.value)} />
          </label>
          <p className={chars > MAX_TEXT_CHARS ? "note error" : "sub"}>
            {chars} of {MAX_TEXT_CHARS} characters
          </p>
          <div className="dialog-buttons">
            <button type="submit" className="btn primary" disabled={!valid}>
              Review
            </button>
            <button type="button" className="btn" onClick={onClose}>
              Cancel
            </button>
          </div>
        </form>
      )}
      {(step === "confirm" || step === "busy") && (
        <>
          <p>
            Send this {type} to <code>{to}</code> in team <code>{team}</code>?
          </p>
          <pre className="text">{text}</pre>
          <div className="dialog-buttons">
            <button className="btn primary" onClick={send} disabled={step === "busy"} autoFocus>
              {step === "busy" ? "Sending…" : `Send ${type}`}
            </button>
            <button className="btn" onClick={() => setStep("write")} disabled={step === "busy"}>
              Back
            </button>
          </div>
        </>
      )}
      {step === "done" && outcome && (
        <>
          <OutcomeLine outcome={outcome} />
          <div className="dialog-buttons">
            <button className="btn" onClick={onClose} autoFocus>
              Close
            </button>
          </div>
        </>
      )}
    </ActionDialog>
  );
}

/** What the CLI printed, both streams, for the user to read. */
function CliOutput({ result }: { result: ActionResult }) {
  const text = [result.stdout, result.stderr].filter((t) => t !== "").join("\n");
  return text === "" ? null : <pre className="text">{text}</pre>;
}

function LeadNotes({ result }: { result: ActionResult }) {
  const lines = leadNoteLines(result.leadNotes);
  if (lines.length === 0) return null;
  return (
    <ul className="results">
      {result.leadNotes.map((n, i) => (
        <li key={i} className={n.told ? undefined : "note error"}>
          {lines[i]}
        </li>
      ))}
    </ul>
  );
}

const FLAG_LABEL: Record<CloseFlag, string> = {
  stop: "--stop: also close it while it is working",
  unverified: "--unverified: also close it when no verify has passed",
};

/**
 * Closes one member: a dry run first, then the real close with the flags the
 * dry run used. The parent keeps it open after the member leaves the page.
 */
export function CloseMemberDialog({ team, role, onClose }: { team: string; role: string; onClose: () => void }) {
  const client = useClient();
  const [flags, setFlags] = useState<CloseFlags>({ stop: false, unverified: false });
  const [preview, setPreview] = useState<{ flags: CloseFlags; result: ActionResult } | null>(null);
  const [confirmed, setConfirmed] = useState<CloseFlag[]>([]);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<ActionResult | null>(null);
  const [error, setError] = useState<Outcome | null>(null);

  const runPreview = async (f: CloseFlags) => {
    setBusy(true);
    setError(null);
    try {
      setPreview({ flags: f, result: await client.closeMember(team, { role, ...f, dryRun: true }) });
    } catch (e) {
      setError(errorOutcome(e));
    }
    setBusy(false);
  };
  useEffect(() => {
    void runPreview({ stop: false, unverified: false });
  }, []);

  const toggle = (f: CloseFlag) => {
    // A changed flag needs a new dry run before the real close.
    setFlags({ ...flags, [f]: !flags[f] });
    setPreview(null);
    setConfirmed((c) => c.filter((x) => x !== f));
  };
  const close = async () => {
    if (preview === null || !canClose(preview, flags, confirmed)) return;
    setBusy(true);
    setError(null);
    try {
      setDone(await client.closeMember(team, { role, ...preview.flags, dryRun: false }));
    } catch (e) {
      setError(errorOutcome(e));
    }
    setBusy(false);
  };

  if (done) {
    return (
      <ActionDialog title={`Close member ${role}`} busy={false} onClose={onClose}>
        <OutcomeLine outcome={closeOutcome(done, team, false)} />
        <CliOutput result={done} />
        <LeadNotes result={done} />
        <div className="dialog-buttons">
          <button className="btn" onClick={onClose} autoFocus>
            Close
          </button>
        </div>
      </ActionDialog>
    );
  }
  const risks = preview?.result.exit === 0 ? closeRisks(flags) : [];
  return (
    <ActionDialog title={`Close member ${role}`} busy={busy} onClose={onClose}>
      <p className="sub">
        Removes <code>{role}</code> from team <code>{team}</code>: its tab, its worktree and its background jobs. Its branch is kept.
      </p>
      {(["stop", "unverified"] as const).map((f) => (
        <label key={f} className="check">
          <input type="checkbox" checked={flags[f]} disabled={busy} onChange={() => toggle(f)} />
          <span>{FLAG_LABEL[f]}</span>
        </label>
      ))}
      {preview === null && !busy && (
        <div className="dialog-buttons">
          <button className="btn" onClick={() => void runPreview(flags)}>
            Preview with these flags
          </button>
        </div>
      )}
      {busy && <p className="note">Running…</p>}
      {preview && (
        <>
          <h3>Dry run</h3>
          <OutcomeLine outcome={closeOutcome(preview.result, team, true)} />
          <CliOutput result={preview.result} />
        </>
      )}
      {risks.map((r) => (
        <label key={r.flag} className="check">
          <input
            type="checkbox"
            checked={confirmed.includes(r.flag)}
            disabled={busy}
            onChange={(e) => setConfirmed((c) => (e.target.checked ? [...c, r.flag] : c.filter((x) => x !== r.flag)))}
          />
          <span>
            I understand: {r.text}
          </span>
        </label>
      ))}
      {error && <OutcomeLine outcome={error} />}
      <div className="dialog-buttons">
        <button className="btn danger" disabled={busy || preview === null || !canClose(preview, flags, confirmed)} onClick={() => void close()}>
          Close {role}
        </button>
        <button className="btn" onClick={onClose} disabled={busy}>
          Cancel
        </button>
      </div>
    </ActionDialog>
  );
}

/**
 * Closes the whole team: a dry run first, then the real close once the user
 * types the team's name. The server tells the lead before it closes. A
 * whole-team close has no --stop or --unverified, so a busy or unverified
 * member shows as the CLI's refusal.
 */
export function CloseTeamDialog({ team, onClose }: { team: string; onClose: () => void }) {
  const client = useClient();
  const [preview, setPreview] = useState<ActionResult | null>(null);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(true);
  const [done, setDone] = useState<ActionResult | null>(null);
  const [error, setError] = useState<Outcome | null>(null);

  useEffect(() => {
    client.closeTeam(team, { dryRun: true, confirmName: "" }).then(setPreview, (e: unknown) => setError(errorOutcome(e))).finally(() => setBusy(false));
  }, [client, team]);

  const close = async () => {
    if (!canCloseTeam(preview, typed, team)) return;
    setBusy(true);
    setError(null);
    try {
      setDone(await client.closeTeam(team, { dryRun: false, confirmName: typed }));
    } catch (e) {
      setError(errorOutcome(e));
    }
    setBusy(false);
  };

  if (done) {
    return (
      <ActionDialog title={`Close team ${team}`} busy={false} onClose={onClose}>
        <OutcomeLine outcome={closeOutcome(done, team, false)} />
        <CliOutput result={done} />
        <LeadNotes result={done} />
        <div className="dialog-buttons">
          <button className="btn" onClick={onClose} autoFocus>
            Close
          </button>
        </div>
      </ActionDialog>
    );
  }
  return (
    <ActionDialog title={`Close team ${team}`} busy={busy} onClose={onClose}>
      <p className="sub">
        Closes every member of <code>{team}</code>, archives the team and deletes its state. Branches are kept.
      </p>
      <h3>Dry run</h3>
      {preview === null && busy && <p className="note">Running…</p>}
      {preview && (
        <>
          <OutcomeLine outcome={closeOutcome(preview, team, true)} />
          <CliOutput result={preview} />
        </>
      )}
      {preview?.exit === 0 && (
        <label className="field">
          Type the team's name, {team}, to confirm
          <input type="text" value={typed} disabled={busy} autoComplete="off" spellCheck={false} onChange={(e) => setTyped(e.target.value)} />
        </label>
      )}
      {error && <OutcomeLine outcome={error} />}
      <div className="dialog-buttons">
        <button className="btn danger" disabled={busy || !canCloseTeam(preview, typed, team)} onClick={() => void close()}>
          {busy && preview !== null ? "Closing…" : `Close team ${team}`}
        </button>
        <button className="btn" onClick={onClose} disabled={busy}>
          Cancel
        </button>
      </div>
    </ActionDialog>
  );
}
