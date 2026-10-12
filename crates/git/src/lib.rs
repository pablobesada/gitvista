//! A bounded, cancellable, read-only adapter around the installed Git CLI.
//!
//! Display paths are lossy only at the serialization boundary. Git and filesystem
//! calls always use the original native path, obtained from NUL-delimited output.

mod process;

use gitvista_core::{
    Cancellation, ChangeEntry, DiffContents, DiffKind, DiffMode, DiffResult, EntryKind,
    FileContents, GitBackend, Repository, CONTENT_LIMIT_BYTES,
};
use process::{execute, CommandOutput, ProcessError};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const STATUS_LIMIT: usize = 64 * 1024 * 1024;
const PATCH_LIMIT: usize = 16 * 1024 * 1024;
const QUERY_LIMIT: usize = 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const DISCOVERY_EXCLUSIONS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    ".venv",
    "venv",
    ".cache",
    "build",
    ".next",
    "coverage",
];

#[derive(Debug, Clone)]
pub struct CliGit {
    executable: OsString,
    filters: Arc<Mutex<HashMap<PathBuf, FilterConfiguration>>>,
}

impl Default for CliGit {
    fn default() -> Self {
        Self::new()
    }
}

impl CliGit {
    pub fn new() -> Self {
        Self {
            executable: OsString::from("git"),
            filters: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    /// Mainly useful for testing cancellation and process bounds without invoking a shell.
    pub fn with_executable(executable: impl Into<OsString>) -> Self {
        Self {
            executable: executable.into(),
            filters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn run(
        &self,
        repo: &Path,
        args: Vec<OsString>,
        limit: usize,
        control: &Cancellation,
    ) -> Result<CommandOutput, ProcessError> {
        execute(
            &self.executable,
            repo,
            &args,
            limit,
            COMMAND_TIMEOUT,
            control,
        )
    }
    fn checked(
        &self,
        repo: &Path,
        args: Vec<OsString>,
        limit: usize,
        control: &Cancellation,
    ) -> Result<Vec<u8>, String> {
        let output = self
            .run(repo, args, limit, control)
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(output.error_message())
        }
    }

    pub fn discover(
        &self,
        root: &Path,
        extra: &[PathBuf],
        control: &Cancellation,
    ) -> Result<Vec<Repository>, String> {
        control.check()?;
        let root = fs::canonicalize(root)
            .map_err(|error| format!("Cannot open {}: {error}", root.display()))?;
        if !root.is_dir() {
            return Err("Workspace root must be a directory".into());
        }
        let mut candidates = vec![];
        let mut seen_directories = HashSet::new();
        let mut stack = vec![root.clone()];
        // Explicit additions may live inside an otherwise excluded folder.
        for path in extra {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                root.join(path)
            };
            let absolute = fs::canonicalize(&absolute).map_err(|error| {
                format!(
                    "Cannot open added repository {}: {error}",
                    absolute.display()
                )
            })?;
            if !absolute.starts_with(&root) {
                return Err("Added repositories must be inside the active workspace root".into());
            }
            if !absolute.is_dir() {
                return Err(format!(
                    "Added repository {} is not a directory",
                    absolute.display()
                ));
            }
            if !absolute.join(".git").exists() {
                control.check()?;
                return Err(format!(
                    "{} is not a Git working-tree root",
                    absolute.display()
                ));
            }
            self.inspect_repository(&absolute, &root, control)?;
            candidates.push(absolute.clone());
            stack.push(absolute);
        }
        while let Some(directory) = stack.pop() {
            control.check()?;
            if !seen_directories.insert(directory.clone()) {
                continue;
            }
            if directory.join(".git").exists() {
                candidates.push(directory.clone());
            }
            // Discover nested repositories even when the current folder is itself a repo.
            let entries = fs::read_dir(&directory)
                .map_err(|error| discovery_error("read directory", &directory, error))?;
            for entry in entries {
                control.check()?;
                let entry = entry
                    .map_err(|error| discovery_error("read an entry in", &directory, error))?;
                let file_type = entry
                    .file_type()
                    .map_err(|error| discovery_error("inspect entry", &entry.path(), error))?;
                if !file_type.is_dir() || file_type.is_symlink() {
                    continue;
                }
                if DISCOVERY_EXCLUSIONS
                    .iter()
                    .any(|excluded| entry.file_name() == OsStr::new(excluded))
                {
                    continue;
                }
                stack.push(entry.path());
            }
        }
        // A root opened at a subdirectory still has a containing repository.
        if !root.join(".git").exists() {
            if let Some(ancestor) = root
                .ancestors()
                .skip(1)
                .find(|path| path.join(".git").exists())
            {
                candidates.push(ancestor.to_path_buf());
            }
            control.check()?;
        }
        candidates.sort();
        candidates.dedup();
        let mut repositories = Vec::with_capacity(candidates.len());
        let mut seen_repositories = HashSet::new();
        for candidate in candidates {
            control.check()?;
            let repository = match self.inspect_repository(&candidate, &root, control) {
                Ok(repository) => repository,
                Err(error) => {
                    control.check()?;
                    unavailable_repository(&candidate, &root, &error)
                }
            };
            if seen_repositories.insert(repository.raw_path.clone()) {
                repositories.push(repository);
            }
        }
        control.check()?;
        repositories.sort_by(|a, b| a.raw_path.cmp(&b.raw_path));
        Ok(repositories)
    }

    fn inspect_repository(
        &self,
        directory: &Path,
        root: &Path,
        control: &Cancellation,
    ) -> Result<Repository, String> {
        control.check()?;
        let raw_path = fs::canonicalize(directory).map_err(|error| error.to_string())?;
        let marker = directory.join(".git");
        let raw_git_dir = if marker.is_dir() {
            fs::canonicalize(marker).map_err(|error| error.to_string())?
        } else {
            let bytes = read_small_file(&marker, control)?;
            let target = trim_line_ending(&bytes)
                .strip_prefix(b"gitdir: ")
                .ok_or("Invalid .git file")?;
            resolve_relative_directory(directory, target)?
        };
        let common_marker = raw_git_dir.join("commondir");
        let raw_common_dir = if common_marker.exists() {
            resolve_relative_directory(
                &raw_git_dir,
                trim_line_ending(&read_small_file(&common_marker, control)?),
            )?
        } else {
            raw_git_dir.clone()
        };
        if !raw_common_dir.join("objects").is_dir()
            || (!raw_common_dir.join("refs").is_dir() && !raw_common_dir.join("reftable").is_dir())
        {
            return Err("Invalid Git directory: missing objects or references".into());
        }
        let head = read_small_file(&raw_git_dir.join("HEAD"), control)?;
        let head = trim_line_ending(&head);
        let branch = if let Some(reference) = head.strip_prefix(b"ref: ") {
            if !reference.starts_with(b"refs/")
                || reference
                    .iter()
                    .any(|byte| byte.is_ascii_whitespace() || *byte == 0)
            {
                return Err("Invalid symbolic HEAD".into());
            }
            String::from_utf8_lossy(reference.strip_prefix(b"refs/heads/").unwrap_or(reference))
                .into_owned()
        } else if matches!(head.len(), 40 | 64) && head.iter().all(u8::is_ascii_hexdigit) {
            format!("detached · {}", String::from_utf8_lossy(&head[..7]))
        } else {
            return Err("Invalid Git HEAD".into());
        };
        Ok(Repository {
            id: opaque_id(&native_bytes(raw_path.as_os_str())),
            path: display_path(&raw_path),
            relative_path: repository_relative_path(&raw_path, root),
            relative_path_key: repository_relative_path_key(&raw_path, root),
            git_dir: display_path(&raw_git_dir),
            common_dir: display_path(&raw_common_dir),
            branch,
            raw_path,
            raw_git_dir,
            raw_common_dir,
        })
    }

    pub fn status(
        &self,
        repo: &Repository,
        root: &Path,
        control: &Cancellation,
    ) -> Result<Vec<ChangeEntry>, String> {
        let root = fs::canonicalize(root)
            .map_err(|error| format!("Cannot open workspace root: {error}"))?;
        let mut arguments = self.filter_overrides(repo, control)?;
        arguments.extend(args(&[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ]));
        if root != repo.directory() {
            if let Ok(scope) = root.strip_prefix(repo.directory()) {
                arguments.push("--".into());
                arguments.push(scope.as_os_str().to_owned());
            }
        }
        let bytes = self.checked(repo.directory(), arguments, STATUS_LIMIT, control)?;
        parse_status(&bytes, repo, &root, control)
    }

    pub fn diff(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<DiffResult, String> {
        control.check()?;
        validate_entry(repo, entry)?;
        let mut result = DiffResult {
            entry_id: entry.id.clone(),
            repo_id: repo.id.clone(),
            mode,
            patch: String::new(),
            kind: DiffKind::Empty,
            message: None,
            old_path: entry.old_path.clone().or_else(|| Some(entry.path.clone())),
            new_path: entry.path.clone(),
        };
        if entry.kind != EntryKind::File {
            result.kind = DiffKind::Metadata;
            if entry.kind == EntryKind::Submodule {
                let mut arguments = args(&[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--unified=3",
                    "--submodule=short",
                    "--ignore-submodules=dirty",
                ]);
                match mode {
                    DiffMode::Total if self.has_head(repo, control)? => {
                        arguments.push("HEAD".into())
                    }
                    DiffMode::Total => {
                        // Git synthesizes its own empty tree for unborn HEADs.
                        // hash-object without -w computes its ID without writes,
                        // respecting SHA-1/SHA-256 repositories automatically.
                        let empty_tree = self.checked(
                            repo.directory(),
                            args(&["hash-object", "-t", "tree", "--stdin"]),
                            QUERY_LIMIT,
                            control,
                        )?;
                        arguments.push(native_path(trim_line_ending(&empty_tree)).into_os_string());
                    }
                    DiffMode::Staged => arguments.push("--cached".into()),
                    DiffMode::Unstaged => (),
                }
                arguments.push("--".into());
                arguments.push(entry.file_path().as_os_str().to_owned());
                if mode != DiffMode::Unstaged {
                    if let Some(old) = entry.old_file_path() {
                        arguments.push(old.as_os_str().to_owned());
                    }
                }
                match self.run(repo.directory(), arguments, PATCH_LIMIT, control) {
                    Ok(output) if output.status.success() => {
                        result.patch = String::from_utf8_lossy(&output.stdout).into_owned()
                    }
                    Ok(output) => return Err(output.error_message()),
                    Err(ProcessError::OutputLimit) => {
                        result.kind = DiffKind::TooLarge;
                        result.message =
                            Some("The generated gitlink patch exceeds the display limit.".into());
                        return Ok(result);
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            result.message = Some(
                if entry.kind == EntryKind::Submodule {
                    "Submodule changed. Open its repository to review individual files."
                } else {
                    "Nested repository. Select a changed file inside it."
                }
                .into(),
            );
            return Ok(result);
        }
        if is_conflict(entry) {
            result.kind = DiffKind::Conflict;
            result.message = Some("This file has an unresolved merge conflict. GitVista does not resolve conflicts in this version.".into());
            return Ok(result);
        }
        if let Some(message) = self.filter_limitation(repo, entry.file_path(), control)? {
            result.kind = DiffKind::Metadata;
            result.message = Some(message);
            return Ok(result);
        }
        let sides = self.sides(repo, entry, mode, control)?;
        for side in [&sides.0, &sides.1] {
            match self.side_size(repo, side, control) {
                Ok(Some(size)) if size > CONTENT_LIMIT_BYTES as u64 => {
                    result.kind = DiffKind::TooLarge;
                    result.message =
                        Some("This file exceeds the 10 MiB per-side display limit.".into());
                    return Ok(result);
                }
                Err(error) => return Err(error),
                _ => (),
            }
        }
        if matches!((&sides.0, &sides.1), (Side::Empty, Side::Empty)) {
            return Ok(result);
        }
        let mut arguments = args(&[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--unified=3",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ]);
        let no_index = matches!(&sides.0, Side::Empty) && matches!(&sides.1, Side::Working(_));
        if no_index {
            arguments.push("--no-index".into());
            arguments.push("--".into());
            arguments.push(null_device().into());
            arguments.push(entry.file_path().as_os_str().to_owned());
        } else {
            match mode {
                DiffMode::Total => {
                    if !self.has_head(repo, control)? {
                        return Ok(result);
                    }
                    arguments.push("HEAD".into());
                }
                DiffMode::Staged => {
                    arguments.push("--cached".into());
                }
                DiffMode::Unstaged => (),
            }
            arguments.push("--".into());
            arguments.push(entry.file_path().as_os_str().to_owned());
            if mode != DiffMode::Unstaged {
                if let Some(old_path) = entry.old_file_path() {
                    if old_path != entry.file_path() {
                        arguments.push(old_path.as_os_str().to_owned());
                        // Explicit two-path comparison permits correct rename detection without
                        // searching all repository contents on each file selection.
                        arguments.retain(|arg| arg != OsStr::new("--no-renames"));
                        arguments.insert(1, "--find-renames".into());
                    }
                }
            }
        }
        let output = match self.run(repo.directory(), arguments, PATCH_LIMIT, control) {
            Ok(output) => output,
            Err(ProcessError::OutputLimit) => {
                result.kind = DiffKind::TooLarge;
                result.message =
                    Some("The generated patch exceeds the 16 MiB display limit.".into());
                return Ok(result);
            }
            Err(error) => return Err(error.to_string()),
        };
        if !output.status.success() && !(no_index && output.status.code() == Some(1)) {
            return Err(output.error_message());
        }
        let mut patch_bytes = output.stdout;
        // Rename detection can cease matching after a staged rename receives a
        // large unstaged edit. Preserve the one-selected-file split contract by
        // comparing its known old/new sides directly rather than displaying two
        // independent added/deleted files.
        let removed_from_index_but_present = mode == DiffMode::Total
            && entry.index_status == "D"
            && matches!((&sides.0, &sides.1), (Side::Head(_), Side::Working(_)));
        let split_rename = entry.old_file_path().is_some()
            && mode != DiffMode::Unstaged
            && patch_bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| line.starts_with(b"diff --git "))
                .count()
                > 1;
        if removed_from_index_but_present || split_rename {
            // Git diff HEAD excludes paths removed from the index even if an
            // untracked file remains. Total means HEAD versus the actual file.
            match self.compare_selected_sides(repo, entry, &sides, control) {
                Ok(patch) => patch_bytes = patch,
                Err(ProcessError::OutputLimit) => {
                    result.kind = DiffKind::TooLarge;
                    result.message =
                        Some("The generated patch exceeds the 16 MiB display limit.".into());
                    return Ok(result);
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        result.patch = match String::from_utf8(patch_bytes) {
            Ok(patch) => patch,
            Err(_) => {
                result.kind = DiffKind::Metadata;
                result.message = Some(
                    "This file contains non-UTF-8 text. Its contents cannot be displayed reliably."
                        .into(),
                );
                return Ok(result);
            }
        };
        result.kind = if result.patch.is_empty() {
            DiffKind::Empty
        } else if result
            .patch
            .lines()
            .any(|line| line.starts_with("Binary files ") || line == "GIT binary patch")
        {
            DiffKind::Binary
        } else if result.patch.lines().any(|line| line.starts_with("@@ ")) {
            DiffKind::Text
        } else {
            DiffKind::Metadata
        };
        if result.kind == DiffKind::Binary {
            result.message = Some("Binary file changed. Text comparison is unavailable.".into());
        }
        if result.kind == DiffKind::Metadata {
            result.message =
                Some("Only the file name or mode changed; there are no changed text lines.".into());
        }
        Ok(result)
    }

    pub fn contents(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<DiffContents, String> {
        control.check()?;
        validate_entry(repo, entry)?;
        if entry.kind != EntryKind::File || is_conflict(entry) {
            return Err(
                "Full content is unavailable for repositories, submodules, or unresolved conflicts"
                    .into(),
            );
        }
        if let Some(message) = self.filter_limitation(repo, entry.file_path(), control)? {
            return Err(message);
        }
        let (old, new) = self.sides(repo, entry, mode, control)?;
        let old_name = if mode == DiffMode::Unstaged {
            entry.file_path()
        } else {
            entry.old_file_path().unwrap_or(entry.file_path())
        };
        Ok(DiffContents {
            old_file: self.read_side(repo, &old, old_name, control)?,
            new_file: self.read_side(repo, &new, entry.file_path(), control)?,
            edit_version: None,
            edit_unavailable_reason: None,
        })
    }

    fn has_head(&self, repo: &Repository, control: &Cancellation) -> Result<bool, String> {
        self.run(
            repo.directory(),
            args(&["rev-parse", "--verify", "--quiet", "HEAD"]),
            QUERY_LIMIT,
            control,
        )
        .map(|output| output.status.success())
        .map_err(|error| error.to_string())
    }

    fn sides(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<(Side, Side), String> {
        let new_path = entry.file_path().to_path_buf();
        let old_path = entry
            .old_file_path()
            .unwrap_or(entry.file_path())
            .to_path_buf();
        let in_index = entry.index_status != "?" && entry.index_status != "D";
        let in_head = entry.index_status != "?"
            && entry.index_status != "A"
            && !(entry.index_status == "." && entry.worktree_status == "A")
            && self.has_head(repo, control)?;
        let working = if entry.worktree_status == "D" {
            Side::Empty
        } else {
            match fs::symlink_metadata(repo.directory().join(&new_path)) {
                Ok(_) => Side::Working(new_path.clone()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Side::Empty,
                Err(error) => return Err(format!("Cannot read {}: {error}", new_path.display())),
            }
        };
        Ok(match mode {
            DiffMode::Total => (
                if in_head {
                    Side::Head(old_path)
                } else {
                    Side::Empty
                },
                working,
            ),
            DiffMode::Staged => (
                if in_head {
                    Side::Head(old_path)
                } else {
                    Side::Empty
                },
                if in_index {
                    Side::Index(new_path)
                } else {
                    Side::Empty
                },
            ),
            DiffMode::Unstaged => (
                if in_index {
                    Side::Index(new_path)
                } else {
                    Side::Empty
                },
                working,
            ),
        })
    }

    fn side_size(
        &self,
        repo: &Repository,
        side: &Side,
        control: &Cancellation,
    ) -> Result<Option<u64>, String> {
        control.check()?;
        match side {
            Side::Empty => Ok(None),
            Side::Working(path) => {
                let metadata = fs::symlink_metadata(repo.directory().join(path))
                    .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
                if metadata.file_type().is_symlink() {
                    Ok(Some(
                        native_bytes(
                            fs::read_link(repo.directory().join(path))
                                .map_err(|error| error.to_string())?
                                .as_os_str(),
                        )
                        .len() as u64,
                    ))
                } else {
                    Ok(Some(metadata.len()))
                }
            }
            Side::Head(_) | Side::Index(_) => {
                let output = self.checked(
                    repo.directory(),
                    vec!["cat-file".into(), "-s".into(), blob_spec(side)],
                    QUERY_LIMIT,
                    control,
                )?;
                let size = String::from_utf8_lossy(&output)
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| "Git returned an invalid blob size")?;
                Ok(Some(size))
            }
        }
    }

    fn read_side(
        &self,
        repo: &Repository,
        side: &Side,
        name: &Path,
        control: &Cancellation,
    ) -> Result<Option<FileContents>, String> {
        let Some(size) = self.side_size(repo, side, control)? else {
            return Ok(None);
        };
        if size > CONTENT_LIMIT_BYTES as u64 {
            return Err("This file exceeds the 10 MiB per-side display limit".into());
        }
        let bytes = self.read_side_bytes(repo, side, control)?;
        if bytes.contains(&0) {
            return Err("Binary files cannot reveal additional text context".into());
        }
        let cache_key = format!("{}-{}", repo.id, opaque_id(&bytes));
        let contents = String::from_utf8(bytes)
            .map_err(|_| "Non-UTF-8 files cannot reveal additional text context")?;
        Ok(Some(FileContents {
            name: display_path(name),
            contents,
            cache_key,
        }))
    }

    fn read_side_bytes(
        &self,
        repo: &Repository,
        side: &Side,
        control: &Cancellation,
    ) -> Result<Vec<u8>, String> {
        match side {
            Side::Empty => Ok(Vec::new()),
            Side::Head(_) | Side::Index(_) => self.checked(
                repo.directory(),
                vec!["cat-file".into(), "blob".into(), blob_spec(side)],
                CONTENT_LIMIT_BYTES,
                control,
            ),
            Side::Working(path) => read_working_file(&repo.directory().join(path), control),
        }
    }

    fn compare_selected_sides(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        sides: &(Side, Side),
        control: &Cancellation,
    ) -> Result<Vec<u8>, ProcessError> {
        let mut old =
            tempfile::NamedTempFile::new().map_err(|error| ProcessError::Io(error.to_string()))?;
        let mut new =
            tempfile::NamedTempFile::new().map_err(|error| ProcessError::Io(error.to_string()))?;
        // NamedTempFile removes these bounded scratch buffers on every return,
        // including cancellation. Nothing is written in the user's repository.
        old.write_all(
            &self
                .read_side_bytes(repo, &sides.0, control)
                .map_err(ProcessError::Io)?,
        )
        .map_err(|error| ProcessError::Io(error.to_string()))?;
        new.write_all(
            &self
                .read_side_bytes(repo, &sides.1, control)
                .map_err(ProcessError::Io)?,
        )
        .map_err(|error| ProcessError::Io(error.to_string()))?;
        let mut arguments = args(&[
            "diff",
            "--no-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--unified=3",
            "--",
        ]);
        arguments.push(old.path().as_os_str().to_owned());
        arguments.push(new.path().as_os_str().to_owned());
        let output = self.run(repo.directory(), arguments, PATCH_LIMIT, control)?;
        if !output.status.success() && output.status.code() != Some(1) {
            return Err(ProcessError::Io(output.error_message()));
        }
        let old_name = quote_patch_path(entry.old_file_path().unwrap_or(entry.file_path()), b"a/");
        let new_name = quote_patch_path(entry.file_path(), b"b/");
        let mut result = Vec::with_capacity(output.stdout.len());
        let mut in_hunk = false;
        for line in output.stdout.split_inclusive(|byte| *byte == b'\n') {
            if line.starts_with(b"@@ ") {
                in_hunk = true;
            }
            if !in_hunk && line.starts_with(b"diff --git ") {
                result.extend_from_slice(b"diff --git ");
                result.extend_from_slice(&old_name);
                result.push(b' ');
                result.extend_from_slice(&new_name);
                result.push(b'\n');
            } else if !in_hunk && line.starts_with(b"--- ") {
                result.extend_from_slice(b"--- ");
                result.extend_from_slice(&old_name);
                result.push(b'\n');
            } else if !in_hunk && line.starts_with(b"+++ ") {
                result.extend_from_slice(b"+++ ");
                result.extend_from_slice(&new_name);
                result.push(b'\n');
            } else if !in_hunk && line.starts_with(b"Binary files ") {
                result.extend_from_slice(b"Binary files ");
                result.extend_from_slice(&old_name);
                result.extend_from_slice(b" and ");
                result.extend_from_slice(&new_name);
                result.extend_from_slice(b" differ\n");
            } else {
                result.extend_from_slice(line);
            }
        }
        Ok(result)
    }

    fn filter_limitation(
        &self,
        repo: &Repository,
        path: &Path,
        control: &Cancellation,
    ) -> Result<Option<String>, String> {
        let output = self.checked(
            repo.directory(),
            vec![
                "check-attr".into(),
                "-z".into(),
                "filter".into(),
                "working-tree-encoding".into(),
                "--".into(),
                path.as_os_str().to_owned(),
            ],
            QUERY_LIMIT,
            control,
        )?;
        let fields: Vec<_> = output.split(|byte| *byte == 0).collect();
        for attributes in fields.as_chunks::<3>().0 {
            let attribute = String::from_utf8_lossy(attributes[1]);
            let value = String::from_utf8_lossy(attributes[2]);
            if value != "unspecified" && value != "unset" {
                return Ok(Some(format!("This file uses Git {attribute} ({value}). Filtered/LFS and alternate-encoding files are unavailable in this read-only MVP.")));
            }
        }
        Ok(None)
    }

    fn filter_overrides(
        &self,
        repo: &Repository,
        control: &Cancellation,
    ) -> Result<Vec<OsString>, String> {
        let key = repo.directory().to_path_buf();
        if let Some(configuration) = self.filters.lock().unwrap().get(&key) {
            if configuration.cacheable
                && configuration.git_dir == repo.git_directory()
                && configuration.common_dir == repo.common_directory()
                && configuration
                    .files
                    .iter()
                    .all(|(path, stamp)| file_stamp(path) == *stamp)
            {
                return Ok(configuration.overrides.clone());
            }
        }
        // `status` can call clean/process filters too. Discover their names
        // without running them, then explicitly disable each configured driver.
        // Values remain private and are never included in error messages/cache.
        let output = self.checked(
            repo.directory(),
            args(&["config", "--null", "--show-origin", "--list"]),
            QUERY_LIMIT,
            control,
        )?;
        let fields: Vec<_> = output.split(|byte| *byte == 0).collect();
        let mut watched = default_config_paths(repo);
        let mut drivers = HashSet::new();
        let mut cacheable = true;
        for [origin, record] in fields.as_chunks::<2>().0 {
            let origin = origin.strip_prefix(b"file:").map(native_path).map(|path| {
                if path.is_absolute() {
                    path
                } else {
                    repo.directory().join(path)
                }
            });
            if let Some(origin) = &origin {
                watched.insert(origin.clone());
            }
            let name = record.split(|byte| *byte == b'\n').next().unwrap_or(record);
            if name.starts_with(b"filter.") {
                for suffix in [b".clean".as_slice(), b".process", b".required", b".smudge"] {
                    if let Some(driver) = name.strip_suffix(suffix) {
                        drivers.insert(driver.to_vec());
                    }
                }
            }
            if name.starts_with(b"include") && name.ends_with(b".path") {
                let value = record
                    .splitn(2, |byte| *byte == b'\n')
                    .nth(1)
                    .unwrap_or_default();
                if let Some(path) = include_config_path(value, origin.as_deref()) {
                    watched.insert(path);
                } else {
                    cacheable = false;
                }
            }
        }
        let mut drivers: Vec<_> = drivers.into_iter().collect();
        drivers.sort();
        let mut overrides = vec![];
        for driver in drivers {
            for suffix in [
                b".clean=".as_slice(),
                b".process=",
                b".smudge=",
                b".required=false",
            ] {
                let mut setting = driver.clone();
                setting.extend_from_slice(suffix);
                overrides.push("-c".into());
                overrides.push(native_path(&setting).into_os_string());
            }
        }
        let configuration = FilterConfiguration {
            git_dir: repo.git_directory().to_path_buf(),
            common_dir: repo.common_directory().to_path_buf(),
            overrides: overrides.clone(),
            files: watched
                .into_iter()
                .map(|path| {
                    let stamp = file_stamp(&path);
                    (path, stamp)
                })
                .collect(),
            cacheable,
        };
        let mut cache = self.filters.lock().unwrap();
        // Bound metadata even when a user opens thousands of unrelated workspaces.
        let bytes = configuration.estimated_bytes();
        if bytes > 128 * 1024 {
            return Ok(overrides);
        }
        if (cache.len() >= 512 && !cache.contains_key(&key))
            || cache
                .values()
                .map(FilterConfiguration::estimated_bytes)
                .sum::<usize>()
                + bytes
                > 4 * 1024 * 1024
        {
            cache.clear();
        }
        cache.insert(key, configuration);
        Ok(overrides)
    }
}

#[derive(Debug, Clone)]
struct FilterConfiguration {
    git_dir: PathBuf,
    common_dir: PathBuf,
    overrides: Vec<OsString>,
    files: Vec<(PathBuf, Option<(u64, std::time::SystemTime)>)>,
    cacheable: bool,
}

impl FilterConfiguration {
    fn estimated_bytes(&self) -> usize {
        self.git_dir.as_os_str().len()
            + self.common_dir.as_os_str().len()
            + self
                .files
                .iter()
                .map(|(path, _)| path.as_os_str().len() + 64)
                .sum::<usize>()
            + self
                .overrides
                .iter()
                .map(|value| value.len() + 32)
                .sum::<usize>()
    }
}

fn file_stamp(path: &Path) -> Option<(u64, std::time::SystemTime)> {
    fs::metadata(path).ok().map(|metadata| {
        (
            metadata.len(),
            metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
        )
    })
}

fn default_config_paths(repo: &Repository) -> HashSet<PathBuf> {
    let mut paths = HashSet::from([
        repo.directory().join(".git"),
        repo.git_directory().join("HEAD"),
        repo.git_directory().join("commondir"),
        repo.common_directory().join("config"),
        repo.git_directory().join("config.worktree"),
    ]);
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.insert(home.join(".gitconfig"));
        paths.insert(home.join(".config/git/config"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        paths.insert(PathBuf::from(xdg).join("git/config"));
    }
    for variable in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
        if let Some(path) = std::env::var_os(variable) {
            paths.insert(PathBuf::from(path));
        }
    }
    paths.insert(PathBuf::from("/etc/gitconfig"));
    paths
}

fn include_config_path(value: &[u8], origin: Option<&Path>) -> Option<PathBuf> {
    if value.is_empty()
        || value.starts_with(b"%")
        || (value.starts_with(b"~") && !value.starts_with(b"~/"))
    {
        return None;
    }
    if let Some(relative) = value.strip_prefix(b"~/") {
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(native_path(relative)));
    }
    let path = native_path(value);
    if path.is_absolute() {
        Some(path)
    } else {
        origin
            .and_then(Path::parent)
            .map(|directory| directory.join(path))
    }
}

fn read_small_file(path: &Path, control: &Cancellation) -> Result<Vec<u8>, String> {
    const METADATA_LIMIT: usize = 16 * 1024;
    control.check()?;
    let file =
        fs::File::open(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let mut bytes = vec![];
    file.take((METADATA_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > METADATA_LIMIT {
        return Err("Git metadata exceeds its 16 KiB size limit".into());
    }
    control.check()?;
    Ok(bytes)
}
fn trim_line_ending(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}
fn resolve_relative_directory(base: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    if bytes.is_empty() {
        return Err("Empty Git directory path".into());
    }
    let target = native_path(bytes);
    fs::canonicalize(if target.is_absolute() {
        target
    } else {
        base.join(target)
    })
    .map_err(|error| error.to_string())
}
fn repository_relative_path(path: &Path, root: &Path) -> String {
    if root.starts_with(path) {
        String::new()
    } else {
        display_path(path.strip_prefix(root).unwrap_or(path))
    }
}
fn repository_relative_path_key(path: &Path, root: &Path) -> String {
    if root.starts_with(path) {
        String::new()
    } else {
        relative_path_key(path.strip_prefix(root).unwrap_or(path))
    }
}
fn discovery_error(operation: &str, path: &Path, error: std::io::Error) -> String {
    format!("Repository discovery is incomplete: cannot {operation} {}: {error}. Check access to this path and refresh.", path.display())
}
fn unavailable_repository(directory: &Path, root: &Path, error: &str) -> Repository {
    let raw_path = directory.to_path_buf();
    let raw_git_dir = directory.join(".git");
    let raw_common_dir = raw_git_dir.clone();
    Repository {
        id: opaque_id(&native_bytes(directory.as_os_str())),
        path: display_path(directory),
        relative_path: repository_relative_path(directory, root),
        relative_path_key: repository_relative_path_key(directory, root),
        git_dir: display_path(&raw_git_dir),
        common_dir: display_path(&raw_common_dir),
        branch: format!("Unavailable · {error}"),
        raw_path,
        raw_git_dir,
        raw_common_dir,
    }
}

impl GitBackend for CliGit {
    fn discover(
        &self,
        root: &Path,
        extra: &[PathBuf],
        control: &Cancellation,
    ) -> Result<Vec<Repository>, String> {
        self.discover(root, extra, control)
    }
    fn status(
        &self,
        repo: &Repository,
        root: &Path,
        control: &Cancellation,
    ) -> Result<Vec<ChangeEntry>, String> {
        self.status(repo, root, control)
    }
    fn diff(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<DiffResult, String> {
        self.diff(repo, entry, mode, control)
    }
    fn contents(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<DiffContents, String> {
        self.contents(repo, entry, mode, control)
    }
}

#[derive(Debug)]
enum Side {
    Empty,
    Head(PathBuf),
    Index(PathBuf),
    Working(PathBuf),
}

fn blob_spec(side: &Side) -> OsString {
    let (prefix, path) = match side {
        Side::Head(path) => ("HEAD:", path),
        Side::Index(path) => (":", path),
        _ => unreachable!("blob spec only exists for Git objects"),
    };
    let mut spec = OsString::from(prefix);
    spec.push(path.as_os_str());
    spec
}

fn parse_status(
    bytes: &[u8],
    repo: &Repository,
    root: &Path,
    control: &Cancellation,
) -> Result<Vec<ChangeEntry>, String> {
    let mut records = bytes.split(|byte| *byte == 0);
    let mut entries = Vec::new();
    while let Some(record) = records.next() {
        control.check()?;
        if record.is_empty() {
            continue;
        }
        let (path, old_path, xy, submodule) = match record[0] {
            b'1' => {
                let fields: Vec<_> = record.splitn(9, |byte| *byte == b' ').collect();
                if fields.len() != 9 || fields[1].len() != 2 {
                    return Err("Invalid Git status record".into());
                }
                (fields[8], None, fields[1], fields[2].first() == Some(&b'S'))
            }
            b'2' => {
                let fields: Vec<_> = record.splitn(10, |byte| *byte == b' ').collect();
                if fields.len() != 10 || fields[1].len() != 2 {
                    return Err("Invalid Git rename record".into());
                }
                let original = records.next().ok_or("Missing original rename path")?;
                (
                    fields[9],
                    Some(original),
                    fields[1],
                    fields[2].first() == Some(&b'S'),
                )
            }
            b'u' => {
                let fields: Vec<_> = record.splitn(11, |byte| *byte == b' ').collect();
                if fields.len() != 11 || fields[1].len() != 2 {
                    return Err("Invalid Git conflict record".into());
                }
                (
                    fields[10],
                    None,
                    fields[1],
                    fields[2].first() == Some(&b'S'),
                )
            }
            b'?' if record.len() >= 3 => (&record[2..], None, &b"??"[..], false),
            b'!' | b'#' => continue,
            _ => return Err("Unexpected Git status record".into()),
        };
        // Nested repositories are emitted as directory paths with a trailing slash.
        let path = path.strip_suffix(b"/").unwrap_or(path);
        let raw_path = native_path(path);
        validate_relative(&raw_path)?;
        let old_raw_path = old_path.map(native_path);
        if let Some(original) = &old_raw_path {
            validate_relative(original)?;
        }
        let absolute = repo.directory().join(&raw_path);
        if !absolute.starts_with(root) {
            continue;
        }
        let kind = if submodule {
            EntryKind::Submodule
        } else if absolute.join(".git").exists() {
            EntryKind::Repository
        } else {
            EntryKind::File
        };
        let mut identity = native_bytes(repo.directory().as_os_str());
        identity.push(0);
        identity.extend_from_slice(path);
        entries.push(ChangeEntry {
            id: opaque_id(&identity),
            repo_id: repo.id.clone(),
            path: display_path(&raw_path),
            old_path: old_raw_path.as_deref().map(display_path),
            absolute_path: display_path(&absolute),
            relative_path: display_path(absolute.strip_prefix(root).unwrap_or(&absolute)),
            relative_path_key: relative_path_key(absolute.strip_prefix(root).unwrap_or(&absolute)),
            index_status: (xy[0] as char).to_string(),
            worktree_status: (xy[1] as char).to_string(),
            kind,
            raw_path,
            old_raw_path,
        });
    }
    entries.sort_by(|a, b| a.raw_path.cmp(&b.raw_path));
    // `git rm --cached file` can yield both an index deletion and an untracked
    // record at the same path. Keep one opaque entry with both sides' state.
    entries.dedup_by(|later, retained| {
        if later.raw_path != retained.raw_path {
            return false;
        }
        if later.index_status == "?" {
            retained.worktree_status = "?".into();
        } else if retained.index_status == "?" {
            retained.index_status = later.index_status.clone();
            retained.old_path = later.old_path.clone();
            retained.old_raw_path = later.old_raw_path.clone();
        }
        true
    });
    Ok(entries)
}

fn validate_entry(repo: &Repository, entry: &ChangeEntry) -> Result<(), String> {
    if entry.repo_id != repo.id {
        return Err("Selected file belongs to another repository".into());
    }
    validate_relative(entry.file_path())?;
    if let Some(path) = entry.old_file_path() {
        validate_relative(path)?;
    }
    Ok(())
}
fn validate_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        Err("Invalid relative Git file path".into())
    } else {
        Ok(())
    }
}
fn is_conflict(entry: &ChangeEntry) -> bool {
    entry.index_status == "U"
        || entry.worktree_status == "U"
        || matches!(
            (entry.index_status.as_str(), entry.worktree_status.as_str()),
            ("A", "A") | ("D", "D")
        )
}
fn read_working_file(path: &Path, control: &Cancellation) -> Result<Vec<u8>, String> {
    control.check()?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return fs::read_link(path)
            .map(|target| native_bytes(target.as_os_str()))
            .map_err(|error| error.to_string());
    }
    if !metadata.is_file() {
        return Err("Selected path is not a regular text file".into());
    }
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::with_capacity((metadata.len() as usize).min(CONTENT_LIMIT_BYTES));
    let mut chunk = [0u8; 64 * 1024];
    loop {
        control.check()?;
        let length = file.read(&mut chunk).map_err(|error| error.to_string())?;
        if length == 0 {
            break;
        }
        if bytes.len() + length > CONTENT_LIMIT_BYTES {
            return Err("File grew beyond the 10 MiB per-side display limit".into());
        }
        bytes.extend_from_slice(&chunk[..length]);
    }
    Ok(bytes)
}
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn relative_path_key(path: &Path) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut key = String::new();
    for (index, component) in path.components().enumerate() {
        if index > 0 {
            key.push('/');
        }
        for byte in native_bytes(component.as_os_str()) {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                key.push(byte as char);
            } else {
                key.push('%');
                key.push(HEX[(byte >> 4) as usize] as char);
                key.push(HEX[(byte & 15) as usize] as char);
            }
        }
    }
    key
}
fn opaque_id(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
fn quote_patch_path(path: &Path, prefix: &[u8]) -> Vec<u8> {
    let mut result = vec![b'"'];
    for byte in prefix.iter().copied().chain(native_bytes(path.as_os_str())) {
        match byte {
            b'"' | b'\\' => {
                result.push(b'\\');
                result.push(byte);
            }
            b'\n' => result.extend_from_slice(b"\\n"),
            b'\r' => result.extend_from_slice(b"\\r"),
            b'\t' => result.extend_from_slice(b"\\t"),
            32..=126 => result.push(byte),
            byte => result.extend_from_slice(format!("\\{byte:03o}").as_bytes()),
        }
    }
    result.push(b'"');
    result
}
#[cfg(unix)]
fn native_path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes.to_vec()).into()
}
#[cfg(not(unix))]
fn native_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).as_ref())
}
#[cfg(unix)]
fn native_bytes(value: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes().to_vec()
}
#[cfg(not(unix))]
fn native_bytes(value: &OsStr) -> Vec<u8> {
    value.as_encoded_bytes().to_vec()
}
#[cfg(windows)]
fn null_device() -> &'static str {
    "NUL"
}
#[cfg(not(windows))]
fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_retain_the_original_bytes_and_unique_ids() {
        use std::os::unix::ffi::OsStrExt;
        let repo = Repository {
            id: "test".into(),
            path: "/tmp/example".into(),
            relative_path: "example".into(),
            relative_path_key: "example".into(),
            git_dir: "/tmp/example/.git".into(),
            common_dir: "/tmp/example/.git".into(),
            branch: "main".into(),
            raw_path: "/tmp/example".into(),
            raw_git_dir: "/tmp/example/.git".into(),
            raw_common_dir: "/tmp/example/.git".into(),
        };
        let entries = parse_status(
            b"? bad\xff.txt\0? bad\xfe.txt\0",
            &repo,
            Path::new("/tmp"),
            &Cancellation::new(),
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, entries[1].path);
        assert_ne!(entries[0].id, entries[1].id);
        assert_ne!(entries[0].relative_path_key, entries[1].relative_path_key);
        assert_eq!(entries[0].relative_path_key, "example/bad%FE.txt");
        assert_eq!(
            entries[0].file_path().as_os_str().as_bytes(),
            b"bad\xfe.txt"
        );
        let encoded = serde_json::to_string(&entries).unwrap();
        assert!(!encoded.contains("rawPath"));
        assert!(encoded.contains("relativePathKey"));
    }
    #[cfg(unix)]
    #[test]
    fn native_keys_preserve_ancestor_identity_and_repository_ownership() {
        let root = Path::new("/tmp/workspace");
        let repo = unavailable_repository(root, root, "fixture");
        let entries = parse_status(
            b"? dir\xff/file.txt\0? dir\xfe/file.txt\0? dir%FF/file.txt\0",
            &repo,
            root,
            &Cancellation::new(),
        )
        .unwrap();
        let invalid: Vec<_> = entries
            .iter()
            .filter(|entry| entry.relative_path.contains('\u{fffd}'))
            .collect();
        assert_eq!(invalid[0].relative_path, invalid[1].relative_path);
        assert_ne!(
            invalid[0].relative_path_key.split('/').next(),
            invalid[1].relative_path_key.split('/').next()
        );
        let literal = entries
            .iter()
            .find(|entry| entry.relative_path == "dir%FF/file.txt")
            .unwrap();
        assert_eq!(literal.relative_path_key, "dir%25FF/file.txt");
        for entry in invalid {
            let directory = root.join(entry.file_path().parent().unwrap());
            let child = unavailable_repository(&directory, root, "fixture");
            assert_eq!(
                format!("{}/file.txt", child.relative_path_key),
                entry.relative_path_key
            );
        }
        assert_eq!(relative_path_key(Path::new("src/main.rs")), "src/main.rs");
        assert_eq!(
            relative_path_key(Path::new("café/file name")),
            "caf%C3%A9/file%20name"
        );
    }
}
