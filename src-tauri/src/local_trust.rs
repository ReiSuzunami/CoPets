use std::{
    fs::{File, Metadata, OpenOptions},
    io,
    path::Path,
};

use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use tokio::net::UnixStream;

fn effective_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and does not dereference pointers.
    unsafe { libc::geteuid() }
}

fn require_owner(metadata: &Metadata, expected_uid: u32) -> io::Result<()> {
    if metadata.uid() != expected_uid {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "local Codex source is owned by another user",
        ));
    }
    Ok(())
}

fn validate_socket_metadata(metadata: &Metadata, expected_uid: u32) -> io::Result<()> {
    if !metadata.file_type().is_socket() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local Codex IPC path is not a Unix socket",
        ));
    }
    require_owner(metadata, expected_uid)
}

fn validate_regular_metadata(metadata: &Metadata, expected_uid: u32) -> io::Result<()> {
    if !metadata.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "local Codex evidence path is not a regular file",
        ));
    }
    require_owner(metadata, expected_uid)
}

fn validate_directory_chain(
    mut current: Option<&Path>,
    allow_root_symlink: bool,
) -> io::Result<()> {
    let expected_uid = effective_uid();
    while let Some(directory) = current {
        let metadata = std::fs::symlink_metadata(directory)?;
        let owner = metadata.uid();
        let root_owned_symlink =
            allow_root_symlink && owner == 0 && metadata.file_type().is_symlink();
        if !metadata.file_type().is_dir() && !root_owned_symlink {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "local Codex source has an unsafe parent path",
            ));
        }
        if owner != 0 && owner != expected_uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "local Codex source parent is owned by another user",
            ));
        }
        if !root_owned_symlink && metadata.permissions().mode() & 0o022 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "local Codex source parent is group- or world-writable",
            ));
        }
        current = directory.parent();
    }
    Ok(())
}

pub(crate) fn validate_protected_parent_chain(path: &Path) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "local Codex source has no parent directory",
        )
    })?;
    validate_directory_chain(Some(parent), true)?;
    let resolved_parent = std::fs::canonicalize(parent)?;
    validate_directory_chain(Some(&resolved_parent), false)
}

pub(crate) fn validate_owned_socket(path: &Path) -> io::Result<()> {
    validate_protected_parent_chain(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    validate_socket_metadata(&metadata, effective_uid())
}

pub(crate) fn validate_same_user_peer(stream: &UnixStream) -> io::Result<()> {
    let peer = stream.peer_cred()?;
    if peer.uid() != effective_uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "local Codex IPC peer is owned by another user",
        ));
    }
    Ok(())
}

pub(crate) fn owned_regular_metadata(path: &Path) -> io::Result<Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    validate_regular_metadata(&metadata, effective_uid())?;
    Ok(metadata)
}

pub(crate) fn open_owned_regular(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    validate_regular_metadata(&file.metadata()?, effective_uid())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::{
        effective_uid, open_owned_regular, owned_regular_metadata, validate_protected_parent_chain,
        validate_same_user_peer, validate_socket_metadata,
    };
    use std::{
        fs,
        os::unix::{
            fs::{PermissionsExt, symlink},
            net::UnixListener as StdUnixListener,
        },
    };
    use tokio::net::UnixStream;

    fn test_root() -> std::path::PathBuf {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let root = std::env::temp_dir().join(format!("dpt-{}", &id[..8]));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn rejects_non_socket_and_foreign_socket_owner_policy() {
        let root = test_root();
        let file = root.join("not-a-socket");
        fs::write(&file, b"not a socket").unwrap();
        let file_metadata = fs::symlink_metadata(&file).unwrap();
        assert!(validate_socket_metadata(&file_metadata, effective_uid()).is_err());

        let socket = root.join("ipc.sock");
        let _listener = StdUnixListener::bind(&socket).unwrap();
        let socket_metadata = fs::symlink_metadata(&socket).unwrap();
        assert!(validate_socket_metadata(&socket_metadata, effective_uid()).is_ok());
        assert!(validate_socket_metadata(&socket_metadata, effective_uid() + 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_symlinked_evidence_before_open() {
        let root = test_root();
        let target = root.join("target.jsonl");
        let link = root.join("link.jsonl");
        fs::write(&target, b"safe\n").unwrap();
        symlink(&target, &link).unwrap();

        assert!(owned_regular_metadata(&link).is_err());
        assert!(open_owned_regular(&link).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_writable_parent_chain_for_path_based_consumers() {
        let root = test_root();
        let writable = root.join("writable");
        fs::create_dir(&writable).unwrap();
        fs::set_permissions(&writable, fs::Permissions::from_mode(0o777)).unwrap();

        assert!(validate_protected_parent_chain(&writable.join("state.sqlite")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_user_owned_parent_symlink() {
        let root = test_root();
        let target = root.join("target");
        let link = root.join("link");
        fs::create_dir(&target).unwrap();
        symlink(&target, &link).unwrap();

        assert!(validate_protected_parent_chain(&link.join("state.sqlite")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn accepts_same_user_connected_peer() {
        let (left, _right) = UnixStream::pair().unwrap();
        validate_same_user_peer(&left).unwrap();
    }
}
