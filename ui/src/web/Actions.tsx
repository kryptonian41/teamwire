// The write actions: messages as the user, and closing a member or the team.
// Each one asks for a confirm first and shows the CLI's result plainly. The
// page never retries an action by itself.

import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { MAX_TEXT_CHARS } from "../shared/api.ts";
import { ApiError } from "./api.ts";
import { useClient } from "./hooks.ts";
import { sendOutcome, type Outcome } from "./model/actions.ts";

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
