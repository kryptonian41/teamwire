//! The human user's key (ADR 0040): 64 lowercase hex characters and a
//! newline in `<home>/user.key`, private to the OS user. `teamwire send
//! --from user` is accepted only from a process that holds it.

use std::path::{Path, PathBuf};

use teamwire_core::Platform;
use teamwire_store::user_key_path;

/// The key file under `home`, made when it is missing or empty. A file that
/// already holds a key is kept, so every process that read it keeps
/// working. Anything at the path that is not a regular file, a symlink
/// included, is refused rather than followed. Returns the file's path; the
/// key itself is never returned. Two runs at once agree on one key: each
/// holds the user-key lock from its first look at the file to its write.
pub fn ensure_user_key(platform: &dyn Platform, home: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt as _;
    let path = user_key_path(home);
    let shown = path.display();
    let _lock = lock_user_key(home)?;
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_file() => {
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("cannot read {shown}: {e}"))?;
            if !text.trim().is_empty() {
                return Ok(path);
            }
            // An empty file is a make that stopped before the key was written.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("cannot make {shown} private: {e}"))?;
        }
        Ok(_) => return Err(format!("{shown} is not a regular file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create_private(&path)?,
        Err(e) => return Err(format!("cannot read {shown}: {e}")),
    }
    let key = crate::member::new_token()?;
    platform
        .write_atomic(&path, format!("{key}\n").as_bytes())
        .map_err(|e| format!("cannot write {shown}: {e}"))?;
    Ok(path)
}

/// The kernel's `flock` on `<home>/locks/user-key.lock`, a file made if
/// missing and never deleted. Without it a second run could find the first
/// run's empty file and write a key over the one the first run then wrote.
/// The lock ends with the open file, so a run that is killed holds nothing.
#[expect(
    clippy::disallowed_methods,
    reason = "the user-key lock file is only ever locked, never written"
)]
fn lock_user_key(home: &Path) -> Result<std::fs::File, String> {
    let locks = home.join("locks");
    std::fs::create_dir_all(&locks).map_err(|e| format!("cannot make {}: {e}", locks.display()))?;
    let path = locks.join("user-key.lock");
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("cannot open the lock {}: {e}", path.display()))?;
    lock.lock()
        .map_err(|e| format!("cannot take the lock {}: {e}", path.display()))?;
    Ok(lock)
}

/// A new empty file with mode 0600 at `path`. `write_atomic` keeps the mode
/// of the file it replaces, so the key is private from its first byte.
/// `create_new` never follows a symlink planted at the path.
fn create_private(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt as _;
    #[expect(
        clippy::disallowed_methods,
        reason = "an empty placeholder fixes the mode that write_atomic then keeps"
    )]
    let made = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path);
    made.map(drop)
        .map_err(|e| format!("cannot make {}: {e}", path.display()))
}
