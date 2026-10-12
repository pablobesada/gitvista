//! Explicit working-file writes. Git queries and the Git index remain read-only.
use gitvista_core::{ChangeEntry, Repository};

#[cfg(unix)]
mod native {
    use super::*;
    use gitvista_core::{EntryKind, CONTENT_LIMIT_BYTES};
    use sha2::{Digest, Sha256};
    use std::{
        ffi::{CString, OsStr},
        fs::{File, Metadata, OpenOptions},
        io::{Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
        path::Component,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    const CHANGED: &str = "El archivo cambió en disco. Tu borrador se conserva; cancelá y actualizá para revisar la versión nueva antes de guardar.";

    fn name(value: &OsStr) -> Result<CString, String> {
        CString::new(value.as_bytes()).map_err(|_| "Ruta inválida".into())
    }
    fn open_at(parent: &File, name: &CString, flags: i32) -> Result<File, String> {
        // Each native component is resolved through an owned directory descriptor.
        // O_NOFOLLOW prevents symlink traversal, including a concurrently replaced parent.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(format!(
                "No se pudo abrir el archivo sin seguir enlaces: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn target(repo: &Repository, entry: &ChangeEntry) -> Result<(File, CString), String> {
        if entry.kind != EntryKind::File {
            return Err(
                "Sólo se pueden editar archivos de texto del directorio de trabajo.".into(),
            );
        }
        let mut components = Vec::new();
        for component in entry.file_path().components() {
            let Component::Normal(value) = component else {
                return Err("Ruta de archivo inválida".into());
            };
            if value.as_bytes().eq_ignore_ascii_case(b".git") {
                return Err("No se pueden editar metadatos de Git.".into());
            }
            components.push(name(value)?);
        }
        let filename = components.pop().ok_or("Ruta vacía")?;
        let mut parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(repo.directory())
            .map_err(|e| e.to_string())?;
        for component in components {
            parent = open_at(&parent, &component, libc::O_RDONLY | libc::O_DIRECTORY)?;
        }
        Ok((parent, filename))
    }
    fn read(parent: &File, filename: &CString) -> Result<(Vec<u8>, Metadata), String> {
        let mut file = open_at(parent, filename, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("El archivo no es un archivo regular.".into());
        }
        if metadata.len() > CONTENT_LIMIT_BYTES as u64 {
            return Err("El archivo supera el límite de 10 MiB.".into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        Read::by_ref(&mut file)
            .take(CONTENT_LIMIT_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > CONTENT_LIMIT_BYTES
            || bytes.contains(&0)
            || std::str::from_utf8(&bytes).is_err()
        {
            return Err("La edición requiere texto UTF-8 de hasta 10 MiB.".into());
        }
        if metadata.mode() & 0o222 == 0 {
            return Err("El archivo está marcado como sólo lectura.".into());
        }
        Ok((bytes, metadata))
    }
    fn version(bytes: &[u8], metadata: &Metadata) -> String {
        let mut hash = Sha256::new();
        hash.update(format!(
            "{}:{}:{}:{}:{}:",
            metadata.dev(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.mode()
        ));
        hash.update(bytes);
        format!("{:x}", hash.finalize())
    }
    pub fn inspect(
        repo: &Repository,
        entry: &ChangeEntry,
        expected_contents: &str,
    ) -> Result<String, String> {
        let (parent, filename) = target(repo, entry)?;
        let (bytes, metadata) = read(&parent, &filename)?;
        if bytes != expected_contents.as_bytes() {
            return Err(CHANGED.into());
        }
        Ok(version(&bytes, &metadata))
    }
    struct Temporary<'a> {
        parent: &'a File,
        name: CString,
    }
    impl Drop for Temporary<'_> {
        fn drop(&mut self) {
            unsafe {
                libc::unlinkat(self.parent.as_raw_fd(), self.name.as_ptr(), 0);
            }
        }
    }
    pub fn save(
        repo: &Repository,
        entry: &ChangeEntry,
        expected: &str,
        contents: &str,
    ) -> Result<(), String> {
        if contents.len() > CONTENT_LIMIT_BYTES || contents.as_bytes().contains(&0) {
            return Err("La edición requiere texto UTF-8 de hasta 10 MiB.".into());
        }
        let (parent, filename) = target(repo, entry)?;
        let (bytes, metadata) = read(&parent, &filename)?;
        if version(&bytes, &metadata) != expected {
            return Err(CHANGED.into());
        }
        let temp_name = CString::new(format!(
            ".gitvista-save-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ))
        .unwrap();
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                temp_name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let temporary = Temporary {
            parent: &parent,
            name: temp_name,
        };
        let mut file = unsafe { File::from_raw_fd(fd) };
        file.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
        file.set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        // Recheck after writing the temporary file; never silently overwrite an external edit.
        let (current, current_metadata) = read(&parent, &filename)?;
        if version(&current, &current_metadata) != expected {
            return Err(CHANGED.into());
        }
        let result = unsafe {
            libc::renameat(
                parent.as_raw_fd(),
                temporary.name.as_ptr(),
                parent.as_raw_fd(),
                filename.as_ptr(),
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use gitvista_core::EntryKind;
        use std::{
            fs,
            os::unix::fs::{symlink, PermissionsExt},
            path::PathBuf,
        };
        fn fixture() -> (tempfile::TempDir, Repository, ChangeEntry) {
            let dir = tempfile::tempdir().unwrap();
            let repo = Repository {
                id: "repo".into(),
                path: dir.path().to_string_lossy().into(),
                relative_path: ".".into(),
                relative_path_key: ".".into(),
                git_dir: String::new(),
                common_dir: String::new(),
                branch: "main".into(),
                raw_path: dir.path().into(),
                raw_git_dir: PathBuf::new(),
                raw_common_dir: PathBuf::new(),
            };
            let entry = ChangeEntry {
                id: "file".into(),
                repo_id: "repo".into(),
                path: "á ' file.txt".into(),
                old_path: None,
                absolute_path: String::new(),
                relative_path: String::new(),
                relative_path_key: String::new(),
                index_status: ".".into(),
                worktree_status: "M".into(),
                kind: EntryKind::File,
                raw_path: PathBuf::from("á ' file.txt"),
                old_raw_path: None,
            };
            fs::write(repo.directory().join(entry.file_path()), "old\r\n").unwrap();
            (dir, repo, entry)
        }
        #[test]
        fn atomic_save_preserves_bytes_permissions_and_cleans_temporary_files() {
            let (_dir, repo, entry) = fixture();
            let path = repo.directory().join(entry.file_path());
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            let token = inspect(&repo, &entry, "old\r\n").unwrap();
            save(&repo, &entry, &token, "new\r\n").unwrap();
            assert_eq!(fs::read(&path).unwrap(), b"new\r\n");
            assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o755);
            assert_eq!(fs::read_dir(repo.directory()).unwrap().count(), 1);
        }
        #[test]
        fn external_edits_and_replacements_are_never_overwritten() {
            let (_dir, repo, entry) = fixture();
            let token = inspect(&repo, &entry, "old\r\n").unwrap();
            fs::write(repo.directory().join(entry.file_path()), "external\n").unwrap();
            assert!(save(&repo, &entry, &token, "draft\n").is_err());
            assert_eq!(
                fs::read(repo.directory().join(entry.file_path())).unwrap(),
                b"external\n"
            );
            assert!(inspect(&repo, &entry, "old\r\n").is_err());
        }
        #[test]
        fn symlinks_traversal_git_metadata_and_oversized_writes_are_rejected() {
            let (dir, repo, mut entry) = fixture();
            let outside = tempfile::tempdir().unwrap();
            fs::write(outside.path().join("file"), "outside").unwrap();
            symlink(outside.path(), dir.path().join("link")).unwrap();
            entry.raw_path = PathBuf::from("link/file");
            assert!(inspect(&repo, &entry, "outside").is_err());
            entry.raw_path = PathBuf::from("../file");
            assert!(inspect(&repo, &entry, "outside").is_err());
            entry.raw_path = PathBuf::from(".git/config");
            assert!(inspect(&repo, &entry, "outside").is_err());
            assert!(save(
                &repo,
                &entry,
                "invalid",
                &"x".repeat(CONTENT_LIMIT_BYTES + 1)
            )
            .is_err());
            assert_eq!(fs::read(outside.path().join("file")).unwrap(), b"outside");
        }
    }
}
#[cfg(unix)]
pub use native::{inspect, save};

#[cfg(not(unix))]
pub fn inspect(
    _repo: &Repository,
    _entry: &ChangeEntry,
    _contents: &str,
) -> Result<String, String> {
    Err("La edición todavía requiere macOS o Unix.".into())
}
#[cfg(not(unix))]
pub fn save(
    _repo: &Repository,
    _entry: &ChangeEntry,
    _expected: &str,
    _contents: &str,
) -> Result<(), String> {
    Err("La edición todavía requiere macOS o Unix.".into())
}
