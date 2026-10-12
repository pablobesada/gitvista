//! GitVista's application-independent domain and resource limits.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const CACHE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
pub const CONTENT_LIMIT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum DiffMode {
    Total,
    Unstaged,
    Staged,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiffKind {
    Text,
    Binary,
    Metadata,
    Conflict,
    Empty,
    TooLarge,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Repository,
    Submodule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub path: String,
    pub relative_path: String,
    /// Slash-delimited percent-encoded native components; identity, not a label.
    pub relative_path_key: String,
    pub git_dir: String,
    pub common_dir: String,
    pub branch: String,
    /// Never reconstruct a Git argument from the display string.
    #[serde(skip)]
    pub raw_path: PathBuf,
    #[serde(skip)]
    pub raw_git_dir: PathBuf,
    #[serde(skip)]
    pub raw_common_dir: PathBuf,
}

impl Repository {
    pub fn directory(&self) -> &Path {
        if self.raw_path.as_os_str().is_empty() {
            Path::new(&self.path)
        } else {
            &self.raw_path
        }
    }
    pub fn git_directory(&self) -> &Path {
        if self.raw_git_dir.as_os_str().is_empty() {
            Path::new(&self.git_dir)
        } else {
            &self.raw_git_dir
        }
    }
    pub fn common_directory(&self) -> &Path {
        if self.raw_common_dir.as_os_str().is_empty() {
            Path::new(&self.common_dir)
        } else {
            &self.raw_common_dir
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub id: String,
    pub repo_id: String,
    pub path: String,
    pub old_path: Option<String>,
    pub absolute_path: String,
    pub relative_path: String,
    /// Shares the repository's relative-root key namespace without lossy decoding.
    pub relative_path_key: String,
    pub index_status: String,
    pub worktree_status: String,
    pub kind: EntryKind,
    #[serde(skip)]
    pub raw_path: PathBuf,
    #[serde(skip)]
    pub old_raw_path: Option<PathBuf>,
}

impl ChangeEntry {
    pub fn file_path(&self) -> &Path {
        if self.raw_path.as_os_str().is_empty() {
            Path::new(&self.path)
        } else {
            &self.raw_path
        }
    }
    pub fn old_file_path(&self) -> Option<&Path> {
        self.old_raw_path
            .as_deref()
            .or_else(|| self.old_path.as_deref().map(Path::new))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    pub repository: Repository,
    pub entries: Vec<ChangeEntry>,
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub root: String,
    pub generation: u64,
    pub repositories: Vec<RepoState>,
    pub scanning: bool,
    pub scanned_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
// Events are short-lived channel payloads. Keeping RepoState by value avoids an
// extra heap allocation for the dominant Repository event and preserves its API.
#[allow(clippy::large_enum_variant)]
pub enum WorkspaceEvent {
    Reset {
        snapshot: WorkspaceSnapshot,
    },
    Repository {
        generation: u64,
        state: RepoState,
        scanned_count: usize,
        total_count: usize,
    },
    Done {
        generation: u64,
        elapsed_ms: u64,
    },
    Error {
        generation: u64,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub entry_id: String,
    pub repo_id: String,
    pub mode: DiffMode,
    pub patch: String,
    pub kind: DiffKind,
    pub message: Option<String>,
    pub old_path: Option<String>,
    pub new_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResponse {
    pub request_id: u64,
    pub generation: u64,
    pub result: DiffResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContents {
    pub name: String,
    pub contents: String,
    pub cache_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffContents {
    pub old_file: Option<FileContents>,
    pub new_file: Option<FileContents>,
    #[serde(default)]
    pub edit_version: Option<EditVersion>,
    #[serde(default)]
    pub edit_unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditVersion {
    pub generation: u64,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub recent_roots: Vec<String>,
    #[serde(default)]
    pub opened_roots: Vec<String>,
    pub extra_repositories: HashMap<String, Vec<String>>,
    pub sidebar_width: f64,
    pub expanded_paths: HashMap<String, Vec<String>>,
    pub last_root: Option<String>,
    #[serde(default)]
    pub tree_view: TreeView,
    #[serde(default)]
    pub untracked_by_repo: HashMap<String, bool>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TreeView {
    #[default]
    Folders,
    Repositories,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            recent_roots: vec![],
            opened_roots: vec![],
            extra_repositories: HashMap::new(),
            sidebar_width: 280.0,
            expanded_paths: HashMap::new(),
            last_root: None,
            tree_view: TreeView::Folders,
            untracked_by_repo: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Operation cancelled".into())
        } else {
            Ok(())
        }
    }
}

pub trait GitBackend: Send + Sync {
    fn discover(
        &self,
        root: &Path,
        extra: &[PathBuf],
        control: &Cancellation,
    ) -> Result<Vec<Repository>, String>;
    fn status(
        &self,
        repo: &Repository,
        root: &Path,
        control: &Cancellation,
    ) -> Result<Vec<ChangeEntry>, String>;
    fn diff(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<DiffResult, String>;
    fn contents(
        &self,
        repo: &Repository,
        entry: &ChangeEntry,
        mode: DiffMode,
        control: &Cancellation,
    ) -> Result<DiffContents, String>;
}

/// A byte-budgeted cache: its limit measures payloads rather than item counts.
/// The application owns admission estimates and can avoid caching one-off huge values.
pub struct ByteLru<K, V> {
    entries: HashMap<K, (V, usize, u64)>,
    byte_budget: usize,
    used_bytes: usize,
    clock: u64,
}

impl<K: Hash + Eq + Clone, V> ByteLru<K, V> {
    pub fn new(byte_budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            byte_budget,
            used_bytes: 0,
            clock: 0,
        }
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.clock = self.clock.wrapping_add(1);
        self.entries.get_mut(key).map(|(value, _, touched)| {
            *touched = self.clock;
            &*value
        })
    }
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        if let Some((_, old_bytes, _)) = self.entries.remove(&key) {
            self.used_bytes -= old_bytes;
        }
        if bytes > self.byte_budget {
            return;
        }
        while self.used_bytes + bytes > self.byte_budget {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, _, clock))| *clock)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                if let Some((_, old_bytes, _)) = self.entries.remove(&oldest) {
                    self.used_bytes -= old_bytes;
                }
            } else {
                break;
            }
        }
        self.clock = self.clock.wrapping_add(1);
        self.used_bytes += bytes;
        self.entries.insert(key, (value, bytes, self.clock));
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.used_bytes = 0;
    }
    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_evicts_by_last_use_and_rejects_oversize() {
        let mut cache = ByteLru::new(8);
        cache.insert("a", 1, 4);
        cache.insert("b", 2, 4);
        assert_eq!(cache.get(&"a"), Some(&1));
        cache.insert("c", 3, 4);
        assert_eq!(cache.get(&"b"), None);
        cache.insert("huge", 4, 9);
        assert_eq!(cache.used_bytes(), 8);
        cache.insert("a", 5, 2);
        assert_eq!(cache.used_bytes(), 6);
        cache.clear();
        assert!(cache.is_empty());
    }
    #[test]
    fn cancellation_clones_share_state() {
        let token = Cancellation::new();
        let other = token.clone();
        other.cancel();
        assert!(token.check().is_err());
    }
}
