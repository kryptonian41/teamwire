//! Reading a harness's screen, shared by every harness adapter: a terminal
//! tail read with its style codes turned back into plain text, the text
//! matching helpers, and the bounded wait for the screen after a start.
//! Every adapter parses a styled screen through `visible`, so each one drops
//! the same codes.
//!
//! Never: sleep. The wait's loop and its sleeps stay in each adapter; this
//! file only counts the steps.

use std::time::Duration;

use crate::harness::HarnessError;
use crate::host::{Host, HostErrorCode};
use crate::names::Place;

/// How many screen lines a startup read takes: more than any terminal is
/// tall, so a tall dialog is read whole. A Claude dialog's escalation lines
/// can sit above the last 40 lines, and a marker that is missed lets the
/// dialog be answered (INV-25).
const STARTUP_TAIL_LINES: u16 = 1000;

/// The timeout of each host call an adapter makes while a member starts.
pub const STARTUP_HOST_TIMEOUT: Duration = Duration::from_secs(5);

/// The text with each run of whitespace made one space, so a phrase that the
/// terminal wrapped onto two lines still matches.
#[must_use]
pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `squashed` holds any of `markers`.
#[must_use]
pub fn has_any(squashed: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| squashed.contains(m))
}

/// How long an adapter's `answer_startup` waits: `steps` reads, `step`
/// apart. A count of sleeps bounds the wait because an adapter does not read
/// the clock there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupPoll {
    pub step: Duration,
    pub steps: u32,
}

impl Default for StartupPoll {
    /// About 30 s in half-second steps.
    fn default() -> Self {
        Self {
            step: Duration::from_millis(500),
            steps: 60,
        }
    }
}

impl StartupPoll {
    /// The steps a read that ran out of `STARTUP_HOST_TIMEOUT` uses up: the
    /// steps that time covers. At most a sixth of the loop, so a loop always
    /// allows six such reads whatever `step` is.
    fn timed_out_steps(self) -> u32 {
        let covered = STARTUP_HOST_TIMEOUT.as_millis() / self.step.as_millis().max(1);
        u32::try_from(covered)
            .unwrap_or(u32::MAX)
            .clamp(1, (self.steps / 6).max(1))
    }

    /// One step's screen, or `None` for "no screen this time": a read that
    /// ran out of its time (a slow start) or that herdr refused because the
    /// agent is working. The caller sleeps and reads again on its next step.
    /// A timed-out read also takes the rest of the steps its 5 s covered out
    /// of `left`, so a loop still ends after about `steps * step` plus one
    /// read. A busy read returns at once and costs nothing more. Any other
    /// host error ends the wait.
    pub fn read(
        self,
        host: &dyn Host,
        place: &Place,
        left: &mut u32,
    ) -> Result<Option<String>, HarnessError> {
        match host.read_tail(place, STARTUP_TAIL_LINES, STARTUP_HOST_TIMEOUT) {
            Ok(screen) => Ok(Some(screen)),
            Err(e) if e.code == HostErrorCode::Timeout => {
                *left = left.saturating_sub(self.timed_out_steps() - 1);
                Ok(None)
            }
            Err(e) if e.code == HostErrorCode::Busy => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// The text of `tail` with the terminal's escape codes taken out, so a
/// screen read with its styles parses as a plain one. With `keep_dim` false,
/// text drawn dim (SGR 2) is dropped too. A plain screen comes back as it was.
pub fn visible(tail: &str, keep_dim: bool) -> String {
    let mut out = String::with_capacity(tail.len());
    let mut dim = false;
    let mut chars = tail.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            if keep_dim || !dim || c == '\n' {
                out.push(c);
            }
            continue;
        }
        match chars.next() {
            // CSI: parameter and intermediate bytes, then one final byte.
            Some('[') => {
                let mut params = String::new();
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        if c == 'm' {
                            dim = sgr_dim(&params, dim);
                        }
                        break;
                    }
                    params.push(c);
                }
            }
            // OSC, such as a hyperlink: up to BEL, or ESC and one more byte.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            // A two-byte escape.
            _ => {}
        }
    }
    out
}

/// Whether text is dim after an SGR sequence with `params`, given `dim`
/// before it.
fn sgr_dim(params: &str, mut dim: bool) -> bool {
    let mut codes = params.split(';');
    while let Some(code) = codes.next() {
        // A colon sub-parameter belongs to its code: `4:3` is code 4.
        match code.split(':').next().unwrap_or_default() {
            "" | "0" | "22" => dim = false,
            "2" => dim = true,
            // An extended colour, `5;n` or `2;r;g;b`, whose `2` is not dim.
            "38" | "48" | "58" if !code.contains(':') => match codes.next() {
                Some("5") => {
                    codes.next();
                }
                Some("2") => {
                    codes.nth(2);
                }
                _ => {}
            },
            _ => {}
        }
    }
    dim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squash_makes_a_wrapped_phrase_one_line() {
        assert_eq!(squash("  Trust this\n   folder?  "), "Trust this folder?");
        assert!(has_any(
            &squash("Trust this\nfolder?"),
            &["x", "this folder"]
        ));
    }

    #[test]
    fn a_timed_out_read_uses_the_steps_its_time_covered_but_at_most_a_sixth() {
        let poll = StartupPoll::default();
        assert_eq!(poll.timed_out_steps(), 10);
        let fast = StartupPoll {
            step: Duration::from_millis(1),
            steps: 12,
        };
        assert_eq!(fast.timed_out_steps(), 2);
        let slow = StartupPoll {
            step: Duration::from_secs(60),
            steps: 3,
        };
        assert_eq!(slow.timed_out_steps(), 1);
    }

    #[test]
    fn a_plain_screen_comes_back_as_it_was() {
        assert_eq!(visible("› hi\n─── \n", false), "› hi\n─── \n");
    }

    #[test]
    fn style_codes_and_links_are_taken_out() {
        let tail = "\u{1b}[38;5;6m──\u{1b}[0m\n\u{1b}]8;;https://x\u{7}link\u{1b}]8;;\u{7} \u{1b}[7m \u{1b}[0m";
        assert_eq!(visible(tail, true), "──\nlink  ");
    }

    #[test]
    fn dim_text_is_dropped_only_when_asked() {
        let tail = "\u{1b}[2mhint\u{1b}[22m typed\n\u{1b}[2mfooter\u{1b}[0m";
        assert_eq!(visible(tail, true), "hint typed\nfooter");
        assert_eq!(visible(tail, false), " typed\n");
    }

    #[test]
    fn the_two_of_a_truecolour_code_is_not_dim() {
        let tail = "\u{1b}[38;2;1;2;3mred\u{1b}[0m";
        assert_eq!(visible(tail, false), "red");
    }
}
