//! A tmux server on a private socket, killed when the value is dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use teamwire_core::{Address, Command, Host, Platform, SpawnRequest};
use teamwire_host_tmux::Tmux;

static NEXT: AtomicU32 = AtomicU32::new(0);

pub(crate) struct PrivateServer {
    pub(crate) socket: String,
    pub(crate) platform: Arc<dyn Platform>,
}

impl PrivateServer {
    /// A socket name no other test uses. No server runs until the first
    /// `spawn`.
    pub(crate) fn new() -> Self {
        Self {
            socket: format!(
                "teamwire-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ),
            platform: teamwire_platform::native(),
        }
    }

    pub(crate) fn tmux(&self) -> Tmux {
        Tmux::new(Arc::clone(&self.platform)).with_socket(self.socket.clone())
    }

    /// Runs `tmux -L <socket> <args>` and returns stdout.
    pub(crate) fn raw(&self, args: &[&str]) -> String {
        let cmd = Command::new("tmux")
            .args(["-L", &self.socket])
            .args(args.iter().copied());
        let out = self.platform.run(&cmd, Duration::from_secs(10)).unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

impl Drop for PrivateServer {
    fn drop(&mut self) {
        let cmd = Command::new("tmux").args(["-L", &self.socket, "kill-server"]);
        drop(self.platform.run(&cmd, Duration::from_secs(10)));
    }
}

pub(crate) fn request(name: &str, dir: &std::path::Path, argv: &[&str]) -> SpawnRequest {
    SpawnRequest {
        name: Address::parse(name).unwrap(),
        title: "test".to_owned(),
        cwd: dir.to_path_buf(),
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        env: Vec::new(),
    }
}

/// Looks at the pane's screen every 50 ms until `text` shows, up to 5 s.
pub(crate) fn screen_shows(host: &dyn Host, place: &teamwire_core::Place, text: &str) -> bool {
    for _ in 0..100 {
        let tail = host.read_tail(place, 50, Duration::from_secs(10)).unwrap();
        if tail.contains(text) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}
