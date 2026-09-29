//! The process tree under a pane's first process, from `ps`, which both
//! macOS and Linux have with the same `-A -o pid= -o ppid=` options.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use teamwire_core::{Command, HostError, HostErrorCode, Pid, Platform};

use crate::call::clip;

/// `root` and every process below it, `root` first, the rest in pid order.
pub(crate) fn tree_of(
    platform: &dyn Platform,
    root: Pid,
    timeout: Duration,
) -> Result<Vec<Pid>, HostError> {
    let cmd = Command::new("ps").args(["-A", "-o", "pid=", "-o", "ppid="]);
    let out = platform.run(&cmd, timeout).map_err(|e| {
        HostError::new(
            HostErrorCode::Other,
            format!("could not run ps: {}", clip(&e.to_string())),
        )
    })?;
    if out.timed_out {
        return Err(HostError::new(
            HostErrorCode::Timeout,
            "ps did not answer in time",
        ));
    }
    if !out.success() {
        return Err(HostError::new(
            HostErrorCode::Other,
            format!("ps exited {}", out.exit_code),
        ));
    }
    Ok(descendants(&String::from_utf8_lossy(&out.stdout), root))
}

/// Parses `pid ppid` lines and walks down from `root`. A line that is not
/// two numbers is skipped.
fn descendants(listing: &str, root: Pid) -> Vec<Pid> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for line in listing.lines() {
        let mut fields = line.split_whitespace().map(str::parse::<u32>);
        if let (Some(Ok(pid)), Some(Ok(ppid))) = (fields.next(), fields.next()) {
            children.entry(ppid).or_default().push(pid);
        }
    }
    let mut seen: HashSet<u32> = HashSet::from([root.get()]);
    let mut order = vec![root.get()];
    let mut next = 0;
    while let Some(&current) = order.get(next) {
        next += 1;
        for child in children.get(&current).into_iter().flatten() {
            if seen.insert(*child) {
                order.push(*child);
            }
        }
    }
    let mut rest = order.split_off(1);
    rest.sort_unstable();
    order.extend(rest);
    order.into_iter().map(Pid::new).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_children_and_grandchildren_but_not_siblings() {
        let listing = "  1 0\n 10 1\n 11 10\n 12 11\n 20 1\n 13 10\n garbage\n";
        let got: Vec<u32> = descendants(listing, Pid::new(10))
            .into_iter()
            .map(Pid::get)
            .collect();
        assert_eq!(got, [10, 11, 12, 13]);
    }

    #[test]
    fn a_pid_with_no_children_is_alone() {
        let got = descendants("1 0\n", Pid::new(7));
        assert_eq!(got, [Pid::new(7)]);
    }

    #[test]
    fn a_loop_in_the_listing_ends() {
        let got = descendants("2 3\n3 2\n", Pid::new(2));
        assert_eq!(got, [Pid::new(2), Pid::new(3)]);
    }
}
