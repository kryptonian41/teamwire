//! Syntax errors: like herdr, a call with the wrong shape prints a usage
//! line on stderr and exits 2, before it reaches the server.

use super::args::Args;

struct Shape {
    command: &'static str,
    usage: &'static str,
    operands: std::ops::RangeInclusive<usize>,
    /// Every flag the real command accepts, value flags and switches.
    flags: &'static [&'static str],
    required_flags: &'static [&'static str],
}

/// The values herdr 0.9.3 accepts for flags that take a fixed set.
const CHOICES: &[(&str, &[&str])] = &[
    (
        "source",
        &["visible", "recent", "recent-unwrapped", "detection"],
    ),
    ("format", &["text", "ansi"]),
    ("until", &["idle", "working", "blocked", "done", "unknown"]),
];

const SHAPES: &[Shape] = &[
    Shape {
        command: "agent start",
        usage: "usage: herdr agent start <name> --kind KIND --pane ID [--timeout MS] [-- <agent-args...>]",
        operands: 1..=1,
        flags: &["kind", "pane", "timeout"],
        required_flags: &["kind", "pane"],
    },
    Shape {
        command: "agent get",
        usage: "usage: herdr agent get <target>",
        operands: 1..=1,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "agent list",
        usage: "usage: herdr agent list",
        operands: 0..=0,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "agent read",
        usage: "usage: herdr agent read <target> [--source SOURCE] [--lines N]",
        operands: 1..=1,
        flags: &["source", "lines", "format", "ansi", "strip-ansi"],
        required_flags: &[],
    },
    Shape {
        command: "agent prompt",
        usage: "usage: herdr agent prompt <target> <text> [--wait] [--until STATUS]... [--timeout MS]",
        operands: 2..=2,
        flags: &["wait", "until", "timeout"],
        required_flags: &[],
    },
    Shape {
        command: "agent send-keys",
        usage: "usage: herdr agent send-keys <target> <key>...",
        operands: 2..=64,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "pane close",
        usage: "usage: herdr pane close <pane>",
        operands: 1..=1,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "pane get",
        usage: "usage: herdr pane get <pane>",
        operands: 1..=1,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "pane list",
        usage: "usage: herdr pane list [--workspace ID]",
        operands: 0..=0,
        flags: &["workspace"],
        required_flags: &[],
    },
    Shape {
        command: "pane rename",
        usage: "usage: herdr pane rename <pane> [label]",
        operands: 1..=2,
        flags: &["clear"],
        required_flags: &[],
    },
    Shape {
        command: "workspace create",
        usage: "usage: herdr workspace create [--cwd PATH] [--label LABEL] [--no-focus]",
        operands: 0..=0,
        flags: &["cwd", "label", "env", "focus", "no-focus"],
        required_flags: &[],
    },
    Shape {
        command: "workspace get",
        usage: "usage: herdr workspace get <workspace>",
        operands: 1..=1,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "workspace list",
        usage: "usage: herdr workspace list",
        operands: 0..=0,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "workspace rename",
        usage: "usage: herdr workspace rename <workspace> <label>",
        operands: 2..=2,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "tab create",
        usage: "usage: herdr tab create --workspace ID [--cwd PATH] [--label LABEL] [--no-focus]",
        operands: 0..=0,
        flags: &["workspace", "cwd", "label", "env", "focus", "no-focus"],
        required_flags: &[],
    },
    Shape {
        command: "tab rename",
        usage: "usage: herdr tab rename <tab> <label>",
        operands: 2..=2,
        flags: &[],
        required_flags: &[],
    },
    Shape {
        command: "fake add-unnamed-agent",
        usage: "usage: herdr fake add-unnamed-agent",
        operands: 0..=0,
        flags: &[],
        required_flags: &[],
    },
];

/// `Err` holds the text to print on stderr before exiting 2.
pub(super) fn check(args: &Args) -> Result<(), String> {
    let command = args.command();
    let Some(shape) = SHAPES.iter().find(|s| s.command == command) else {
        return Err(format!(
            "unknown command: {command}\nrun 'herdr --help' for usage"
        ));
    };
    let operands = args.words.len().saturating_sub(2);
    if !shape.operands.contains(&operands) {
        return Err(shape.usage.to_owned());
    }
    // `--strip-ansi` is not in herdr 0.9.3's help, but the adapter's callers
    // may still send it, so the fake accepts it with `--ansi`.
    if let Some(unknown) = args.flag_names().find(|f| !shape.flags.contains(f)) {
        return Err(format!(
            "unexpected argument '--{unknown}'\n{}",
            shape.usage
        ));
    }
    for (flag, allowed) in CHOICES {
        if let Some(value) = args.value(flag)
            && !allowed.contains(&value)
        {
            return Err(format!(
                "invalid value '{value}' for '--{flag}' [possible values: {}]\n{}",
                allowed.join(", "),
                shape.usage
            ));
        }
    }
    for flag in shape.required_flags {
        if args.value(flag).is_none() {
            return Err(format!("missing required --{flag}"));
        }
    }
    Ok(())
}
