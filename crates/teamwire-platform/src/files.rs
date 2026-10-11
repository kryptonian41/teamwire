//! Files and disks: atomic writes, free space, network file systems.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use nix::sys::statfs::statfs;
use nix::sys::statvfs::statvfs;
use teamwire_core::{PlatformError, PlatformErrorCode};

use crate::io_error;

/// The mode a new file gets when there is no old file to copy one from.
const NEW_FILE_MODE: u32 = 0o644;

/// Write `bytes` to a temporary file in the same directory, sync it, rename
/// it over `path`, then sync the directory so the rename itself survives a
/// crash. A reader sees the old file or the new one, never half of either.
/// The temporary file sits next to the target because a rename only stays
/// atomic inside one file system.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PlatformError> {
    let resolved = through_symlink(path);
    let path = resolved.as_path();
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if path.file_name().is_none() {
        return Err(PlatformError::new(
            PlatformErrorCode::Io,
            format!("{} has no file name", path.display()),
        ));
    }
    let context = |step: &str| format!("{step} {}", path.display());

    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|err| io_error(&context("creating a temporary file for"), &err))?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(|err| io_error(&context("writing"), &err))?;
    set_mode(temp.as_file(), path)
        .map_err(|err| io_error(&context("setting the mode of"), &err))?;
    // On failure the temporary file is removed when `temp` drops.
    temp.persist(path)
        .map_err(|err| io_error(&context("renaming into place"), &err.error))?;
    File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(|err| io_error(&context("syncing the directory of"), &err))
}

/// The file a symlink at `path` points to, so the write replaces the target
/// and the link stays. A path that is not a link, or a link whose target
/// cannot be found yet, is returned as it is; for a dangling link the write
/// then creates the target beside the link's own directory.
fn through_symlink(path: &Path) -> std::path::PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    match fs::read_link(path) {
        Ok(target) => path
            .parent()
            .map_or(target.clone(), |dir| dir.join(&target)),
        Err(_) => path.to_path_buf(),
    }
}

/// Keep the mode of the file being replaced. A temporary file starts private
/// (0600), and a config that was readable would otherwise turn unreadable.
fn set_mode(temp: &File, target: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = match fs::metadata(target) {
        Ok(meta) => meta.permissions().mode() & 0o7777,
        Err(_) => NEW_FILE_MODE,
    };
    temp.set_permissions(fs::Permissions::from_mode(mode))
}

/// Bytes a normal user can still write on the file system holding `path`.
pub(crate) fn free_bytes(path: &Path) -> Result<u64, PlatformError> {
    let stat = statvfs(path).map_err(|errno| errno_error("reading free space of", path, errno))?;
    let blocks = u64::from(stat.blocks_available());
    let unit: u64 = stat.fragment_size();
    Ok(blocks.saturating_mul(unit))
}

/// True when `path` is on a network file system, where `SQLite` locking cannot
/// be trusted (STO-8).
pub(crate) fn on_network_fs(path: &Path) -> Result<bool, PlatformError> {
    let stat =
        statfs(path).map_err(|errno| errno_error("reading the file system of", path, errno))?;
    Ok(is_network(&stat))
}

/// macOS names the file system type in text. These are the network ones:
/// `nfs`, `smbfs`, `afpfs`, `webdav`, `cifs`, `ftp`, `afs` and `autofs` (an
/// automount point, which is normally an NFS mount that is not yet
/// mounted).
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const NETWORK_TYPES: &[&str] = &[
    "nfs", "smbfs", "afpfs", "webdav", "cifs", "ftp", "afs", "autofs",
];

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn is_network(stat: &nix::sys::statfs::Statfs) -> bool {
    NETWORK_TYPES.contains(&stat.filesystem_type_name())
}

/// Linux reports a numeric magic number. These are NFS, SMB, AFS, Coda and
/// NCP; `nix` has no constant for CIFS and SMB2, so those are not detected.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn is_network(stat: &nix::sys::statfs::Statfs) -> bool {
    use nix::sys::statfs::{
        AFS_SUPER_MAGIC, CODA_SUPER_MAGIC, NCP_SUPER_MAGIC, NFS_SUPER_MAGIC, SMB_SUPER_MAGIC,
    };
    let kind = stat.filesystem_type();
    [
        NFS_SUPER_MAGIC,
        SMB_SUPER_MAGIC,
        AFS_SUPER_MAGIC,
        CODA_SUPER_MAGIC,
        NCP_SUPER_MAGIC,
    ]
    .contains(&kind)
}

fn errno_error(what: &str, path: &Path, errno: nix::errno::Errno) -> PlatformError {
    io_error(
        &format!("{what} {}", path.display()),
        &std::io::Error::from(errno),
    )
}
