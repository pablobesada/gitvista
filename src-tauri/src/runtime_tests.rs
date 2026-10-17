//! Integration coverage of real Git processes, IPC snapshots, and filesystem watchers.
//! Every write is confined to a tempfile fixture; no app or user checkout is touched.

use crate::runtime::Runtime;
use gitvista_core::{DiffKind, DiffMode, RepoState, WorkspaceEvent, WorkspaceSnapshot};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::ipc::{Channel, InvokeResponseBody};
use tempfile::TempDir;
use tokio::{
    sync::mpsc,
    time::{timeout_at, Instant},
};

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn repository_display_preferences_migrate_and_round_trip() {
    let fixture = TempDir::new().unwrap();
    let path = fixture.path().join("preferences.json");
    fs::write(&path, r#"{"recentRoots":["/example"],"extraRepositories":{},"sidebarWidth":300,"expandedPaths":{"/example":["src"]},"lastRoot":"/example"}"#).unwrap();
    let runtime = Runtime::new(path.clone());
    let mut preferences = runtime.preferences();
    assert_eq!(preferences.tree_view, gitvista_core::TreeView::Folders);
    assert!(preferences.untracked_by_repo.is_empty());
    assert!(preferences.opened_roots.is_empty());
    preferences.opened_roots = vec!["/example".into(), "/other".into()];
    preferences.tree_view = gitvista_core::TreeView::Repositories;
    preferences.untracked_by_repo.insert("repo-a".into(), false);
    runtime.save_preferences(preferences).unwrap();
    let restored = Runtime::new(path).preferences();
    assert_eq!(restored.tree_view, gitvista_core::TreeView::Repositories);
    assert_eq!(restored.untracked_by_repo.get("repo-a"), Some(&false));
    assert_eq!(restored.last_root.as_deref(), Some("/example"));
    assert_eq!(restored.opened_roots, vec!["/example", "/other"]);
    assert_eq!(restored.expanded_paths["/example"], vec!["src"]);
}

struct Events {
    receiver: mpsc::UnboundedReceiver<WorkspaceEvent>,
    snapshot: Option<WorkspaceSnapshot>,
    seen: Vec<WorkspaceEvent>,
}

impl Events {
    fn channel() -> (Channel<WorkspaceEvent>, Self) {
        Self::channel_with(|_| {})
    }

    fn channel_with(
        on_event: impl Fn(&WorkspaceEvent) + Send + Sync + 'static,
    ) -> (Channel<WorkspaceEvent>, Self) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let channel = Channel::new(move |body| {
            let InvokeResponseBody::Json(json) = body else {
                panic!("Workspace events must be JSON")
            };
            let event: WorkspaceEvent =
                serde_json::from_str(&json).expect("A valid workspace event");
            on_event(&event);
            let _ = sender.send(event);
            Ok(())
        });
        (
            channel,
            Self {
                receiver,
                snapshot: None,
                seen: Vec::new(),
            },
        )
    }

    fn apply(&mut self, event: WorkspaceEvent) {
        match &event {
            WorkspaceEvent::Reset { snapshot } => {
                if self
                    .snapshot
                    .as_ref()
                    .is_none_or(|current| current.generation <= snapshot.generation)
                {
                    self.snapshot = Some(snapshot.clone());
                }
            }
            WorkspaceEvent::Repository {
                generation,
                state,
                scanned_count,
                ..
            } => {
                if let Some(snapshot) = self
                    .snapshot
                    .as_mut()
                    .filter(|snapshot| snapshot.generation == *generation)
                {
                    snapshot
                        .repositories
                        .retain(|repo| repo.repository.id != state.repository.id);
                    snapshot.repositories.push(state.clone());
                    snapshot.scanned_count = *scanned_count;
                }
            }
            WorkspaceEvent::Done { generation, .. } => {
                if let Some(snapshot) = self
                    .snapshot
                    .as_mut()
                    .filter(|snapshot| snapshot.generation == *generation)
                {
                    snapshot.scanning = false;
                }
            }
            WorkspaceEvent::Error {
                generation,
                message,
            } => {
                if self
                    .snapshot
                    .as_ref()
                    .is_none_or(|snapshot| snapshot.generation == *generation)
                {
                    panic!("Runtime event failed: {message}");
                }
            }
        }
        self.seen.push(event);
    }

    async fn wait_for(&mut self, description: &str, predicate: impl Fn(&WorkspaceEvent) -> bool) {
        let deadline = Instant::now() + EVENT_TIMEOUT;
        loop {
            let event = timeout_at(deadline, self.receiver.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "Timed out waiting for {description}; snapshot: {:?}",
                        self.snapshot
                    )
                })
                .expect("Workspace channel stays open");
            let matches = predicate(&event);
            self.apply(event);
            if matches {
                return;
            }
        }
    }

    async fn done(&mut self) {
        let generation = self.snapshot.as_ref().map(|snapshot| snapshot.generation);
        self.wait_for("the scan completion", |event| matches!(event,
            WorkspaceEvent::Done { generation: completed, .. } if generation.is_none_or(|expected| expected == *completed))).await;
    }

    async fn drain_for(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while let Ok(Some(event)) = timeout_at(deadline, self.receiver.recv()).await {
            self.apply(event);
        }
    }

    fn repositories(&self) -> &[RepoState] {
        &self
            .snapshot
            .as_ref()
            .expect("A reset snapshot arrived")
            .repositories
    }

    fn repository(&self, path: &Path) -> &RepoState {
        let path = fs::canonicalize(path).unwrap();
        self.repositories()
            .iter()
            .find(|state| state.repository.directory() == path)
            .expect("Repository exists in snapshot")
    }
}

fn git(path: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(path)
        .args([
            "--no-pager",
            "-c",
            "user.name=GitVista Test",
            "-c",
            "user.email=gitvista-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .output()
        .expect("Git starts for the temporary fixture");
    assert!(
        output.status.success(),
        "Git {:?} failed: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository(path: &Path) -> PathBuf {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q", "-b", "main"]);
    fs::write(path.join("change.txt"), "baseline\n").unwrap();
    git(path, &["add", "--", "change.txt"]);
    git(path, &["commit", "-q", "-m", "fixture baseline"]);
    fs::write(path.join("change.txt"), "initial change\n").unwrap();
    fs::canonicalize(path).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dormant_root_badges_find_ignored_nested_repos_and_cache_only_summary() {
    let fixture = TempDir::new().unwrap();
    let active = repository(&fixture.path().join("active"));
    let dormant = repository(&fixture.path().join("dormant"));
    fs::write(dormant.join("change.txt"), "baseline\n").unwrap();
    fs::write(dormant.join(".gitignore"), "nested/\nnode_modules/\n").unwrap();
    git(&dormant, &["add", ".gitignore"]);
    git(&dormant, &["commit", "-q", "-m", "ignore nested fixtures"]);
    let nested = repository(&dormant.join("nested"));
    let index = fs::read(dormant.join(".git/index")).unwrap();
    let config = fs::read(dormant.join(".git/config")).unwrap();
    let nested_index = fs::read(nested.join(".git/index")).unwrap();
    let runtime = Runtime::new(fixture.path().join("preferences.json"));
    let (channel, mut events) = Events::channel();
    runtime
        .open(active.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    events.done().await;
    let generation = events.snapshot.as_ref().unwrap().generation;
    let roots = vec![dormant.to_string_lossy().into_owned()];
    assert_eq!(
        runtime.root_badges(roots.clone(), false).await.unwrap()[0].has_changes,
        Some(true)
    );
    fs::write(nested.join("change.txt"), "baseline\n").unwrap();
    fs::write(dormant.join("untracked.txt"), "new\n").unwrap();
    fs::write(nested.join("untracked.txt"), "new\n").unwrap();
    // Focus within the TTL needs no new Git work, even if the filesystem changed.
    assert_eq!(
        runtime.root_badges(roots.clone(), false).await.unwrap()[0].has_changes,
        Some(true)
    );
    assert_eq!(
        runtime.root_badges(roots.clone(), true).await.unwrap()[0].has_changes,
        Some(false)
    );
    let excluded = repository(&dormant.join("node_modules/explicit"));
    assert_eq!(
        runtime.root_badges(roots.clone(), true).await.unwrap()[0].has_changes,
        Some(false)
    );
    let mut preferences = runtime.preferences();
    preferences.extra_repositories.insert(
        roots[0].clone(),
        vec![excluded.to_string_lossy().into_owned()],
    );
    runtime.save_preferences(preferences).unwrap();
    assert_eq!(
        runtime.root_badges(roots, true).await.unwrap()[0].has_changes,
        Some(true)
    );
    assert_eq!(fs::read(dormant.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(dormant.join(".git/config")).unwrap(), config);
    assert_eq!(fs::read(nested.join(".git/index")).unwrap(), nested_index);
    events.drain_for(Duration::from_millis(100)).await;
    assert_eq!(events.snapshot.as_ref().unwrap().generation, generation);
    assert_eq!(events.repositories().len(), 1);
    assert_eq!(events.repository(&active).entries.len(), 1);
    runtime.close().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn badge_errors_are_unknown_and_subdirectory_roots_are_scoped() {
    let fixture = TempDir::new().unwrap();
    let parent = repository(&fixture.path().join("parent"));
    let child = parent.join("folder");
    fs::create_dir_all(&child).unwrap();
    let runtime = Runtime::new(fixture.path().join("preferences.json"));
    let roots = vec![child.to_string_lossy().into_owned()];
    let badge = runtime
        .root_badges(roots.clone(), true)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        badge.has_changes,
        Some(false),
        "The changed parent file is outside this root"
    );
    fs::write(child.join("untracked"), "new\n").unwrap();
    assert_eq!(
        runtime.root_badges(roots.clone(), true).await.unwrap()[0].has_changes,
        Some(false),
        "Untracked-only entries do not light the tab"
    );
    git(&parent, &["add", "folder/untracked"]);
    assert_eq!(
        runtime.root_badges(roots, true).await.unwrap()[0].has_changes,
        Some(true),
        "Adding the file to the index lights the tab"
    );
    let missing = runtime
        .root_badges(
            vec![fixture
                .path()
                .join("missing")
                .to_string_lossy()
                .into_owned()],
            true,
        )
        .await
        .unwrap()
        .remove(0);
    assert_eq!(missing.has_changes, None);
    assert!(missing.error.is_some());
}

fn assert_incremental_preserves_all(events: &Events, from: usize, count: usize, changed: &Path) {
    let refresh = &events.seen[from..];
    assert!(refresh.iter().any(|event| matches!(event, WorkspaceEvent::Reset { snapshot } if snapshot.repositories.len() == count)),
        "Incremental scan must publish the complete repository snapshot");
    let updated: Vec<_> = refresh
        .iter()
        .filter_map(|event| match event {
            WorkspaceEvent::Repository {
                state, total_count, ..
            } => Some((state.repository.directory(), *total_count)),
            _ => None,
        })
        .collect();
    assert!(
        !updated.is_empty(),
        "A watcher update must refresh a repository"
    );
    assert!(
        updated
            .iter()
            .all(|(path, total)| *path == changed && *total == 1),
        "Only the affected repository is queried: {updated:?}"
    );
    assert_eq!(
        events.repositories().len(),
        count,
        "Unchanged repositories remain available"
    );
}

#[tokio::test]
async fn closing_last_root_releases_its_events_and_entries_and_allows_reopening() {
    let fixture = TempDir::new().unwrap();
    let root = repository(&fixture.path().join("project"));
    let runtime = Runtime::new(fixture.path().join("config/preferences.json"));
    let (channel, mut events) = Events::channel();
    let opened = runtime
        .open(root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    events.snapshot = Some(opened.clone());
    events.done().await;
    let entry = events.repositories()[0].entries[0].id.clone();
    runtime
        .diff(entry.clone(), DiffMode::Total, 1)
        .await
        .unwrap();
    events.drain_for(Duration::from_millis(700)).await;
    // Closing while a scan is pending must also cancel its publication.
    runtime.refresh(true).await.unwrap();
    runtime.close().unwrap();
    // Reset and any events already published before Close are legitimate.
    while let Ok(event) = events.receiver.try_recv() {
        events.apply(event);
    }
    let before = events.seen.len();
    assert!(runtime.preferences().last_root.is_none());
    assert!(runtime.diff(entry, DiffMode::Total, 2).await.is_err());
    fs::write(root.join("change.txt"), "edited while closed\n").unwrap();
    runtime.refresh(true).await.unwrap();
    events.drain_for(Duration::from_millis(700)).await;
    assert_eq!(events.seen.len(), before);
    let (channel, mut reopened_events) = Events::channel();
    let reopened = runtime
        .open(root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    assert!(reopened.generation > opened.generation);
    reopened_events.snapshot = Some(reopened);
    reopened_events.done().await;
    let entry = reopened_events.repositories()[0].entries[0].id.clone();
    let diff = runtime.diff(entry, DiffMode::Total, 3).await.unwrap();
    assert!(diff.result.patch.contains("edited while closed"));
}

#[tokio::test]
async fn restoring_the_index_version_keeps_staged_changes_and_index_bytes() {
    let fixture = TempDir::new().unwrap();
    let root = repository(&fixture.path().join("project"));
    git(&root, &["add", "--", "change.txt"]);
    fs::write(root.join("change.txt"), "unstaged changes\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let config = fs::read(root.join(".git/config")).unwrap();
    let runtime = Runtime::new(fixture.path().join("config/preferences.json"));
    let (channel, mut events) = Events::channel();
    events.snapshot = Some(
        runtime
            .open(root.to_string_lossy().into_owned(), channel)
            .await
            .unwrap(),
    );
    events.done().await;
    events.drain_for(Duration::from_millis(700)).await;
    let entry = events.repositories()[0].entries[0].id.clone();
    runtime
        .diff(entry.clone(), DiffMode::Unstaged, 1)
        .await
        .unwrap();
    let contents = runtime
        .contents(entry.clone(), DiffMode::Unstaged, 1)
        .await
        .unwrap();
    let original = contents.old_file.unwrap().contents;
    assert_eq!(original, "initial change\n");
    runtime
        .save_file(entry.clone(), contents.edit_version.unwrap(), original)
        .await
        .unwrap();
    events.done().await;
    events.drain_for(Duration::from_millis(700)).await;
    assert_eq!(
        fs::read(root.join("change.txt")).unwrap(),
        b"initial change\n"
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    let staged = runtime
        .diff(entry.clone(), DiffMode::Staged, 2)
        .await
        .unwrap();
    assert!(staged.result.patch.contains("+initial change"));
    let unstaged = runtime.diff(entry, DiffMode::Unstaged, 3).await.unwrap();
    assert_eq!(unstaged.result.kind, DiffKind::Empty);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[tokio::test]
async fn explicit_save_checks_disk_version_and_root_and_never_writes_the_index() {
    let fixture = TempDir::new().unwrap();
    let root = repository(&fixture.path().join("project"));
    let runtime = Runtime::new(fixture.path().join("config/preferences.json"));
    let (channel, mut events) = Events::channel();
    let opened = runtime
        .open(root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    events.snapshot = Some(opened);
    events.done().await;
    events.drain_for(Duration::from_millis(700)).await;
    let entry = events.repositories()[0].entries[0].id.clone();
    let index = fs::read(root.join(".git/index")).unwrap();
    let config = fs::read(root.join(".git/config")).unwrap();
    runtime
        .diff(entry.clone(), DiffMode::Total, 700)
        .await
        .unwrap();
    let version = runtime
        .contents(entry.clone(), DiffMode::Total, 700)
        .await
        .unwrap()
        .edit_version
        .unwrap();
    fs::write(root.join("change.txt"), "external edit\n").unwrap();
    assert!(runtime
        .save_file(entry.clone(), version, "draft\n".into())
        .await
        .is_err());
    assert_eq!(
        fs::read(root.join("change.txt")).unwrap(),
        b"external edit\n"
    );
    events.drain_for(Duration::from_millis(700)).await;
    runtime
        .diff(entry.clone(), DiffMode::Staged, 701)
        .await
        .unwrap();
    assert!(runtime
        .contents(entry.clone(), DiffMode::Staged, 701)
        .await
        .unwrap()
        .edit_version
        .is_none());
    runtime
        .diff(entry.clone(), DiffMode::Total, 702)
        .await
        .unwrap();
    let version = runtime
        .contents(entry.clone(), DiffMode::Total, 702)
        .await
        .unwrap()
        .edit_version
        .unwrap();
    runtime
        .save_file(
            entry.clone(),
            version.clone(),
            "saved by GitVista\r\n".into(),
        )
        .await
        .unwrap();
    events.done().await;
    assert_eq!(
        fs::read(root.join("change.txt")).unwrap(),
        b"saved by GitVista\r\n"
    );
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(".git/config")).unwrap(), config);
    assert!(runtime
        .diff(entry.clone(), DiffMode::Total, 703)
        .await
        .unwrap()
        .result
        .patch
        .contains("saved by GitVista"));
    let other = repository(&fixture.path().join("other"));
    let (channel, _) = Events::channel();
    runtime
        .open(other.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    assert!(runtime
        .save_file(entry, version, "wrong root\n".into())
        .await
        .is_err());
    assert_eq!(
        fs::read(root.join("change.txt")).unwrap(),
        b"saved by GitVista\r\n"
    );
}

#[tokio::test]
async fn progressive_scan_and_incremental_watch_keep_the_entire_workspace() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("project");
    let first = repository(&root.join("first"));
    let second = repository(&root.join("second"));
    let runtime = Runtime::new(fixture.path().join("config/preferences.json"));
    let (channel, mut events) = Events::channel();
    let opened = runtime
        .open(root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    assert!(opened.scanning);
    events.snapshot = Some(opened.clone());
    events.done().await;
    let progress: Vec<_> = events
        .seen
        .iter()
        .filter_map(|event| match event {
            WorkspaceEvent::Repository {
                generation,
                state,
                scanned_count,
                total_count,
            } if *generation == opened.generation => {
                assert_eq!(*total_count, 2);
                assert!(state.error.is_none());
                Some((state.repository.id.clone(), *scanned_count))
            }
            _ => None,
        })
        .collect();
    assert_eq!(progress.len(), 2);
    assert_eq!(
        progress
            .iter()
            .map(|(id, _)| id)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    assert_eq!(
        progress.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(events.repositories().len(), 2);
    assert_eq!(events.repository(&second).entries.len(), 1);

    // Let FSEvents deliver any fixture-creation batch before measuring this invalidation.
    events.drain_for(Duration::from_millis(1200)).await;
    let from = events.seen.len();
    fs::write(first.join("external.txt"), "an external edit\n").unwrap();
    events.wait_for("the externally created file", |event| matches!(event,
        WorkspaceEvent::Repository { state, .. } if state.repository.directory() == first && state.entries.iter().any(|entry| entry.path == "external.txt"))).await;
    events.done().await;
    assert_incremental_preserves_all(&events, from, 2, &first);

    let entry = events
        .repository(&first)
        .entries
        .iter()
        .find(|entry| entry.path == "external.txt")
        .unwrap()
        .clone();
    let diff = runtime
        .diff(entry.id.clone(), DiffMode::Total, 1)
        .await
        .unwrap();
    assert_eq!(diff.result.kind, DiffKind::Text);
    assert!(diff.result.patch.contains("+an external edit"));
    let contents = runtime
        .contents(entry.id.clone(), DiffMode::Total, 1)
        .await
        .unwrap();
    assert!(contents.old_file.is_none());
    assert_eq!(contents.new_file.unwrap().contents, "an external edit\n");
    let other = events.repository(&second).entries[0].id.clone();
    runtime.diff(other, DiffMode::Total, 2).await.unwrap();
    assert!(
        runtime
            .contents(entry.id, DiffMode::Total, 1)
            .await
            .is_err(),
        "Superseded context requests are cancelled"
    );
}

#[tokio::test]
async fn explicitly_added_repository_in_an_excluded_directory_is_watched() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("project");
    let visible = repository(&root.join("visible"));
    let embedded = repository(&root.join("node_modules/custom-repository"));
    let runtime = Runtime::new(fixture.path().join("config/preferences.json"));
    let (channel, mut events) = Events::channel();
    events.snapshot = Some(
        runtime
            .open(root.to_string_lossy().into_owned(), channel)
            .await
            .unwrap(),
    );
    events.done().await;
    assert_eq!(events.repositories().len(), 1);
    assert_eq!(events.repository(&visible).repository.directory(), visible);
    runtime
        .add_repository(embedded.to_string_lossy().into_owned())
        .await
        .unwrap();
    events
        .wait_for("discovery of the explicit repository", |event| {
            matches!(event,
        WorkspaceEvent::Reset { snapshot } if snapshot.repositories.len() == 2)
        })
        .await;
    events.done().await;
    assert_eq!(events.repositories().len(), 2);
    assert!(runtime
        .preferences()
        .extra_repositories
        .values()
        .flatten()
        .any(|path| Path::new(path) == embedded));
    events.drain_for(Duration::from_millis(1200)).await;
    let from = events.seen.len();
    fs::write(
        embedded.join("watched.txt"),
        "watched inside an excluded folder\n",
    )
    .unwrap();
    events.wait_for("the change in the explicitly added repository", |event| matches!(event,
        WorkspaceEvent::Repository { state, .. } if state.repository.directory() == embedded && state.entries.iter().any(|entry| entry.path == "watched.txt"))).await;
    events.done().await;
    assert_incremental_preserves_all(&events, from, 2, &embedded);
}

#[tokio::test]
async fn writes_during_baseline_are_reconciled_and_root_switch_cancels_old_scans() {
    let fixture = TempDir::new().unwrap();
    let old_root = fixture.path().join("old-project");
    for index in 0..12 {
        repository(&old_root.join(format!("repo-{index:02}")));
    }
    let new_root = fixture.path().join("new-project");
    let current = repository(&new_root.join("current"));
    let runtime = Runtime::new(fixture.path().join("config/preferences.json"));
    let wrote = Arc::new(AtomicBool::new(false));
    let written_repository = Arc::new(Mutex::new(None::<PathBuf>));
    let callback_wrote = wrote.clone();
    let callback_path = written_repository.clone();
    let (channel, mut old_events) = Events::channel_with(move |event| {
        if let WorkspaceEvent::Repository { state, .. } = event {
            if !callback_wrote.swap(true, Ordering::AcqRel) {
                let path = state.repository.directory().to_path_buf();
                // Channel callbacks execute inside publication of the first status, before Done.
                fs::write(
                    path.join("during.txt"),
                    "written during the baseline scan\n",
                )
                .unwrap();
                *callback_path.lock().unwrap() = Some(path);
            }
        }
    });
    let opened = runtime
        .open(old_root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    old_events.snapshot = Some(opened.clone());
    old_events.done().await;
    assert!(wrote.load(Ordering::Acquire));
    let written = written_repository.lock().unwrap().clone().unwrap();
    assert!(
        !old_events
            .repository(&written)
            .entries
            .iter()
            .any(|entry| entry.path == "during.txt"),
        "The test write occurred after this repository's baseline status was captured"
    );
    old_events.wait_for("reconciliation of the edit made during scanning", |event| matches!(event,
        WorkspaceEvent::Repository { state, .. } if state.repository.directory() == written && state.entries.iter().any(|entry| entry.path == "during.txt"))).await;
    old_events.done().await;
    assert_eq!(old_events.repositories().len(), 12);
    let old_entry = old_events.repository(&written).entries[0].id.clone();
    old_events.drain_for(Duration::from_millis(700)).await;

    // A full old-root discovery is still pending when the second root takes ownership.
    runtime.refresh(true).await.unwrap();
    let (channel, mut new_events) = Events::channel();
    let switched = runtime
        .open(new_root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    new_events.snapshot = Some(switched.clone());
    assert!(switched.generation > opened.generation);
    new_events.done().await;
    new_events.drain_for(Duration::from_millis(700)).await;
    runtime.refresh(true).await.unwrap();
    new_events.done().await;
    let snapshot = new_events.snapshot.as_ref().unwrap();
    assert_eq!(
        Path::new(&snapshot.root),
        fs::canonicalize(&new_root).unwrap()
    );
    assert_eq!(snapshot.generation, switched.generation);
    assert_eq!(snapshot.repositories.len(), 1);
    assert_eq!(snapshot.repositories[0].repository.directory(), current);
    assert!(
        runtime.diff(old_entry, DiffMode::Total, 100).await.is_err(),
        "The previous root's entries cannot be selected"
    );
    let entry = snapshot.repositories[0].entries[0].id.clone();
    assert_eq!(
        runtime
            .diff(entry, DiffMode::Total, 101)
            .await
            .unwrap()
            .generation,
        switched.generation
    );
}

#[tokio::test]
async fn repository_cleanup_uses_native_id_guards_generation_and_editor_then_refreshes() {
    let fixture = TempDir::new().unwrap();
    let root = repository(&fixture.path().join("workspace"));
    let nested = repository(&root.join("child"));
    let runtime = Runtime::new(fixture.path().join("preferences.json"));
    let (channel, mut events) = Events::channel();
    runtime
        .open(root.to_string_lossy().into_owned(), channel)
        .await
        .unwrap();
    events.done().await;
    let generation = events.snapshot.as_ref().unwrap().generation;
    let repo_id = events.repository(&nested).repository.id.clone();
    assert!(runtime
        .stash_preview("invented-display-path".into(), generation)
        .await
        .is_err());
    assert!(runtime
        .stash_preview(repo_id.clone(), generation + 1)
        .await
        .is_err());
    let preview = runtime
        .stash_preview(repo_id.clone(), generation)
        .await
        .unwrap();
    runtime.set_editor_active(true);
    assert!(runtime
        .stash_repository(repo_id.clone(), generation, preview.token.clone(), true)
        .await
        .unwrap_err()
        .contains("editar"));
    runtime.set_editor_active(false);
    let result = runtime
        .stash_repository(repo_id, generation, preview.token, true)
        .await
        .unwrap();
    assert!(result.warning.is_none(), "{:?}", result.warning);
    assert!(result.stash_oid.is_some());
    events.done().await;
    assert!(events.repository(&nested).entries.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("change.txt")).unwrap(),
        "initial change\n"
    );
    assert_eq!(
        fs::read_to_string(nested.join("change.txt")).unwrap(),
        "baseline\n"
    );
}
