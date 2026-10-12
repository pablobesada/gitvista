use gitvista_core::{Cancellation, DiffKind, DiffMode, EntryKind, CONTENT_LIMIT_BYTES};
use gitvista_git::CliGit;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};

struct Fixture {
    directory: TempDir,
    git: CliGit,
    control: Cancellation,
}
impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            directory: tempdir().unwrap(),
            git: CliGit::new(),
            control: Cancellation::new(),
        };
        init(fixture.root());
        fixture
    }
    fn root(&self) -> &Path {
        self.directory.path()
    }
    fn repository(&self) -> gitvista_core::Repository {
        self.git
            .discover(self.root(), &[], &self.control)
            .unwrap()
            .into_iter()
            .find(|repo| repo.directory() == self.root().canonicalize().unwrap())
            .unwrap()
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.root().join(name), bytes).unwrap();
    }
    fn commit(&self) {
        git(self.root(), &["add", "--all"]);
        git(self.root(), &["commit", "--quiet", "-m", "fixture"]);
    }
    fn entry(&self, name: &str) -> gitvista_core::ChangeEntry {
        self.git
            .status(&self.repository(), self.root(), &self.control)
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == name)
            .unwrap()
    }
}

fn init(path: &Path) {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", "--quiet", "-b", "main"]);
    git(path, &["config", "user.name", "GitVista Test"]);
    git(path, &["config", "user.email", "gitvista@example.invalid"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    git(path, &["config", "core.autocrlf", "false"]);
    git(path, &["config", "status.renames", "true"]);
}

fn git(path: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8_lossy(&result.stdout).into_owned()
}

#[test]
fn discovery_keeps_nested_repositories_and_explicit_exclusions() {
    let outer = Fixture::new();
    init(&outer.root().join("packages/nested"));
    init(&outer.root().join("node_modules/explicit"));
    init(&outer.root().join("target/skipped"));
    let repositories = outer
        .git
        .discover(outer.root(), &[], &outer.control)
        .unwrap();
    assert_eq!(repositories.len(), 2);
    let repositories = outer
        .git
        .discover(
            outer.root(),
            &[PathBuf::from("node_modules/explicit")],
            &outer.control,
        )
        .unwrap();
    assert_eq!(repositories.len(), 3);
    let status = outer
        .git
        .status(&outer.repository(), outer.root(), &outer.control)
        .unwrap();
    assert!(status
        .iter()
        .any(|entry| entry.path == "packages/nested" && entry.kind == EntryKind::Repository));
}

#[test]
fn worktree_git_files_are_discovered_with_their_common_directory() {
    let fixture = Fixture::new();
    fixture.write("a.txt", "base\n");
    fixture.commit();
    git(
        fixture.root(),
        &["worktree", "add", "--quiet", "-b", "second", "linked"],
    );
    let repositories = fixture
        .git
        .discover(fixture.root(), &[], &fixture.control)
        .unwrap();
    let linked = repositories
        .iter()
        .find(|repo| repo.branch == "second")
        .unwrap();
    assert_ne!(linked.git_directory(), linked.common_directory());
    assert_eq!(
        linked.common_directory(),
        fixture.repository().git_directory()
    );
}

#[test]
fn submodules_are_scanned_independently_and_gitlink_changes_have_metadata() {
    let fixture = Fixture::new();
    let source = Fixture::new();
    fixture.write("base.txt", "base\n");
    fixture.commit();
    source.write("file.txt", "base\n");
    source.commit();
    git(
        fixture.root(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "--quiet",
            source.root().to_str().unwrap(),
            "nested/module",
        ],
    );
    fixture.commit();
    let repositories = fixture
        .git
        .discover(fixture.root(), &[], &fixture.control)
        .unwrap();
    assert_eq!(repositories.len(), 2);
    let child = repositories
        .iter()
        .find(|repo| repo.relative_path == "nested/module")
        .unwrap();
    let old_commit = git(child.directory(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    fs::write(child.directory().join("file.txt"), "modified child\n").unwrap();
    let child_changes = fixture
        .git
        .status(child, fixture.root(), &fixture.control)
        .unwrap();
    assert_eq!(child_changes[0].relative_path, "nested/module/file.txt");
    let parent = fixture.repository();
    assert!(fixture
        .git
        .status(&parent, fixture.root(), &fixture.control)
        .unwrap()
        .is_empty());
    git(child.directory(), &["add", "file.txt"]);
    git(
        child.directory(),
        &[
            "-c",
            "user.name=GitVista Test",
            "-c",
            "user.email=gitvista@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "child",
        ],
    );
    let parent_changes = fixture
        .git
        .status(&parent, fixture.root(), &fixture.control)
        .unwrap();
    let gitlink = parent_changes
        .iter()
        .find(|entry| entry.path == "nested/module")
        .unwrap();
    assert_eq!(gitlink.kind, EntryKind::Submodule);
    let new_commit = git(child.directory(), &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    let diff = fixture
        .git
        .diff(&parent, gitlink, DiffMode::Total, &fixture.control)
        .unwrap();
    assert!(diff
        .patch
        .contains(&format!("-Subproject commit {old_commit}")));
    assert!(diff
        .patch
        .contains(&format!("+Subproject commit {new_commit}")));
    assert_eq!(diff.kind, DiffKind::Metadata);
    let unstaged = fixture
        .git
        .diff(&parent, gitlink, DiffMode::Unstaged, &fixture.control)
        .unwrap();
    assert!(unstaged
        .patch
        .contains(&format!("+Subproject commit {new_commit}")));
    assert!(fixture
        .git
        .diff(&parent, gitlink, DiffMode::Staged, &fixture.control)
        .unwrap()
        .patch
        .is_empty());
    // An unborn parent must compare the empty tree to the actual child HEAD,
    // while the prepared comparison still uses the index's older gitlink.
    git(fixture.root(), &["update-ref", "-d", "HEAD"]);
    let objects_before = git(fixture.root(), &["count-objects", "-v"]);
    let unborn_total = fixture
        .git
        .diff(&parent, gitlink, DiffMode::Total, &fixture.control)
        .unwrap();
    assert!(unborn_total
        .patch
        .contains(&format!("+Subproject commit {new_commit}")));
    let unborn_staged = fixture
        .git
        .diff(&parent, gitlink, DiffMode::Staged, &fixture.control)
        .unwrap();
    assert!(unborn_staged
        .patch
        .contains(&format!("+Subproject commit {old_commit}")));
    assert_eq!(
        git(fixture.root(), &["count-objects", "-v"]),
        objects_before
    );
    assert_eq!(
        fixture
            .git
            .diff(&parent, gitlink, DiffMode::Total, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::Metadata
    );
}

#[test]
fn repository_roots_may_contain_newline_characters() {
    let directory = tempdir().unwrap();
    let root = directory.path().join("newline\nrepo");
    init(&root);
    let backend = CliGit::new();
    let token = Cancellation::new();
    let repositories = backend.discover(&root, &[], &token).unwrap();
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].directory(), root.canonicalize().unwrap());
}

#[test]
fn opening_a_subdirectory_never_returns_changes_outside_it() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.root().join("inside")).unwrap();
    fixture.write("outside.txt", "base\n");
    fixture.write("inside/selected.txt", "base\n");
    fixture.commit();
    fixture.write("outside.txt", "changed outside\n");
    fixture.write("inside/selected.txt", "changed inside\n");
    let root = fixture.root().join("inside");
    let repositories = fixture.git.discover(&root, &[], &fixture.control).unwrap();
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].relative_path, "");
    let entries = fixture
        .git
        .status(&repositories[0], &root, &fixture.control)
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].relative_path, "selected.txt");
    assert!(fixture
        .git
        .discover(&root, &[fixture.root().to_path_buf()], &fixture.control)
        .is_err());
}

#[test]
fn configured_filters_are_disabled_in_status_and_cache_invalidates_included_config() {
    let fixture = Fixture::new();
    fixture.write(".gitattributes", "*.filtered filter=custom\n");
    fixture.write("file.filtered", "base\n");
    fixture.commit();
    let repo = fixture.repository();
    git(fixture.root(), &["config", "include.path", "extra-config"]);
    fixture
        .git
        .status(&repo, fixture.root(), &fixture.control)
        .unwrap();
    let clean = "\"touch filter-ran; cat\"";
    fs::write(
        repo.common_directory().join("extra-config"),
        format!("[filter \"custom\"]\n    clean = {clean}\n    required = true\n"),
    )
    .unwrap();
    fixture.write("file.filtered", "best\n");
    let before_index = fs::read(repo.git_directory().join("index")).unwrap();
    let entries = fixture
        .git
        .status(&repo, fixture.root(), &fixture.control)
        .unwrap();
    let selected = entries
        .iter()
        .find(|entry| entry.path == "file.filtered")
        .unwrap();
    assert_eq!(selected.worktree_status, "M");
    assert!(!fixture.root().join("filter-ran").exists());
    assert_eq!(
        fixture
            .git
            .diff(&repo, selected, DiffMode::Total, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::Metadata
    );
    assert_eq!(
        fs::read(repo.git_directory().join("index")).unwrap(),
        before_index
    );
    assert!(!fixture.root().join("filter-ran").exists());
}

#[test]
fn switching_branches_invalidates_conditional_filter_configuration() {
    let fixture = Fixture::new();
    fixture.write(".gitattributes", "*.filtered filter=conditional\n");
    fixture.write("file.filtered", "base\n");
    fixture.commit();
    git(
        fixture.root(),
        &["config", "branch.autoSetupMerge", "false"],
    );
    git(
        fixture.root(),
        &["config", "includeIf.onbranch:feature.path", "branch-config"],
    );
    let repository = fixture.repository();
    let config = repository.common_directory().join("config");
    let included = repository.common_directory().join("branch-config");
    fs::write(&included, "[filter \"conditional\"]\n    clean = \"touch conditional-filter-ran; cat\"\n    required = true\n").unwrap();
    fixture
        .git
        .status(&repository, fixture.root(), &fixture.control)
        .unwrap();
    let config_stamp = fs::metadata(&config).unwrap().modified().unwrap();
    let included_stamp = fs::metadata(&included).unwrap().modified().unwrap();
    git(fixture.root(), &["checkout", "--quiet", "-b", "feature"]);
    assert_eq!(
        fs::metadata(&config).unwrap().modified().unwrap(),
        config_stamp
    );
    assert_eq!(
        fs::metadata(&included).unwrap().modified().unwrap(),
        included_stamp
    );
    fixture.write("file.filtered", "best\n");
    // Demonstrate that the newly active driver really can execute during Git
    // status, then verify the adapter prevents it with the original cached repo.
    git(
        fixture.root(),
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "--untracked-files=all",
        ],
    );
    let sentinel = fixture.root().join("conditional-filter-ran");
    assert!(sentinel.exists());
    fs::remove_file(&sentinel).unwrap();
    fixture.write("file.filtered", "bust\n");
    let index_before = fs::read(repository.git_directory().join("index")).unwrap();
    let entries = fixture
        .git
        .status(&repository, fixture.root(), &fixture.control)
        .unwrap();
    assert!(entries.iter().any(|entry| entry.path == "file.filtered"));
    assert!(!sentinel.exists());
    assert_eq!(
        fs::read(repository.git_directory().join("index")).unwrap(),
        index_before
    );
}

#[test]
fn corrupt_markers_remain_visible_with_status_errors() {
    let fixture = Fixture::new();
    let invalid = fixture.root().join("broken");
    fs::create_dir_all(&invalid).unwrap();
    fs::write(invalid.join(".git"), "gitdir: missing-dir\n").unwrap();
    let repositories = fixture
        .git
        .discover(fixture.root(), &[], &fixture.control)
        .unwrap();
    let unavailable = repositories
        .iter()
        .find(|repo| repo.directory() == invalid.canonicalize().unwrap())
        .unwrap();
    assert!(unavailable.branch.starts_with("Unavailable"));
    assert!(fixture
        .git
        .status(unavailable, fixture.root(), &fixture.control)
        .is_err());
    assert!(fixture
        .git
        .discover(fixture.root(), &[invalid], &fixture.control)
        .is_err());
}

#[cfg(unix)]
#[test]
fn unreadable_nested_directories_report_incomplete_discovery() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let fixture = Fixture::new();
    let inaccessible = fixture.root().join("blocked");
    init(&inaccessible.join("nested"));
    fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o000)).unwrap();
    let result = fixture.git.discover(fixture.root(), &[], &fixture.control);
    // Restore access before asserting so a failed assertion can still clean up.
    fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o755)).unwrap();
    let error = result.unwrap_err();
    assert!(error.contains("discovery is incomplete"));
    assert!(error.contains("blocked"));
    assert!(error.contains("refresh"));
}

#[test]
fn staged_and_unstaged_changes_can_cancel_in_total_without_writing_git_state() {
    let fixture = Fixture::new();
    fixture.write("file.txt", "base\n");
    fixture.commit();
    fixture.write("file.txt", "prepared\n");
    git(fixture.root(), &["add", "file.txt"]);
    fixture.write("file.txt", "base\n");
    let repo = fixture.repository();
    let index_before = fs::read(repo.git_directory().join("index")).unwrap();
    let config_before = fs::read(repo.common_directory().join("config")).unwrap();
    let entry = fixture.entry("file.txt");
    assert_eq!((&*entry.index_status, &*entry.worktree_status), ("M", "M"));
    let total = fixture
        .git
        .diff(&repo, &entry, DiffMode::Total, &fixture.control)
        .unwrap();
    assert_eq!(total.kind, DiffKind::Empty);
    let staged = fixture
        .git
        .diff(&repo, &entry, DiffMode::Staged, &fixture.control)
        .unwrap();
    assert!(staged.patch.contains("+prepared"));
    let unstaged = fixture
        .git
        .diff(&repo, &entry, DiffMode::Unstaged, &fixture.control)
        .unwrap();
    assert!(unstaged.patch.contains("-prepared") && unstaged.patch.contains("+base"));
    let contents = fixture
        .git
        .contents(&repo, &entry, DiffMode::Staged, &fixture.control)
        .unwrap();
    assert_eq!(contents.old_file.unwrap().contents, "base\n");
    assert_eq!(contents.new_file.unwrap().contents, "prepared\n");
    assert_eq!(
        fs::read(repo.git_directory().join("index")).unwrap(),
        index_before
    );
    assert_eq!(
        fs::read(repo.common_directory().join("config")).unwrap(),
        config_before
    );
    assert!(!repo.git_directory().join("index.lock").exists());
}

#[test]
fn index_deletion_with_a_remaining_untracked_file_is_one_entry() {
    let fixture = Fixture::new();
    fixture.write("file.txt", "base\n");
    fixture.commit();
    git(fixture.root(), &["rm", "--cached", "file.txt"]);
    let repo = fixture.repository();
    let entries = fixture
        .git
        .status(&repo, fixture.root(), &fixture.control)
        .unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!((&*entry.index_status, &*entry.worktree_status), ("D", "?"));
    assert_eq!(
        fixture
            .git
            .diff(&repo, entry, DiffMode::Total, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::Empty
    );
    assert!(fixture
        .git
        .diff(&repo, entry, DiffMode::Staged, &fixture.control)
        .unwrap()
        .patch
        .contains("-base"));
    assert!(fixture
        .git
        .diff(&repo, entry, DiffMode::Unstaged, &fixture.control)
        .unwrap()
        .patch
        .contains("+base"));
}

#[test]
fn unicode_newlines_quotes_and_pathspec_metacharacters_are_literal() {
    let fixture = Fixture::new();
    let paths = [
        "á \"quote\".txt",
        "new\nline.txt",
        ":(glob)*.txt",
        "-option.txt",
        "bracket[0].txt",
    ];
    for path in paths {
        fixture.write(path, "before\n");
    }
    fixture.commit();
    for path in paths {
        fixture.write(path, "after\n");
    }
    let repo = fixture.repository();
    let entries = fixture
        .git
        .status(&repo, fixture.root(), &fixture.control)
        .unwrap();
    assert_eq!(entries.len(), paths.len());
    for path in paths {
        let entry = entries.iter().find(|entry| entry.path == path).unwrap();
        let patch = fixture
            .git
            .diff(&repo, entry, DiffMode::Total, &fixture.control)
            .unwrap();
        assert_eq!(patch.kind, DiffKind::Text);
        assert!(patch.patch.contains("-before") && patch.patch.contains("+after"));
        let contents = fixture
            .git
            .contents(&repo, entry, DiffMode::Total, &fixture.control)
            .unwrap();
        assert_eq!(contents.new_file.unwrap().contents, "after\n");
    }
}

#[test]
fn unborn_head_supports_all_three_comparisons() {
    let fixture = Fixture::new();
    fixture.write("new.txt", "prepared\n");
    git(fixture.root(), &["add", "new.txt"]);
    fixture.write("new.txt", "working\n");
    let repo = fixture.repository();
    let entry = fixture.entry("new.txt");
    assert_eq!(repo.branch, "main");
    for (mode, expected) in [
        (DiffMode::Total, "+working"),
        (DiffMode::Staged, "+prepared"),
        (DiffMode::Unstaged, "+working"),
    ] {
        let diff = fixture
            .git
            .diff(&repo, &entry, mode, &fixture.control)
            .unwrap();
        assert_eq!(diff.kind, DiffKind::Text);
        assert!(diff.patch.contains(expected));
    }
    let content = fixture
        .git
        .contents(&repo, &entry, DiffMode::Total, &fixture.control)
        .unwrap();
    assert!(content.old_file.is_none());
    assert_eq!(content.new_file.unwrap().contents, "working\n");
}

#[test]
fn intent_to_add_has_no_head_side_and_no_prepared_patch() {
    let fixture = Fixture::new();
    fixture.write("base.txt", "base\n");
    fixture.commit();
    fixture.write("intent.txt", "new content\n");
    git(fixture.root(), &["add", "--intent-to-add", "intent.txt"]);
    let repo = fixture.repository();
    let entry = fixture.entry("intent.txt");
    assert_eq!((&*entry.index_status, &*entry.worktree_status), (".", "A"));
    assert!(fixture
        .git
        .diff(&repo, &entry, DiffMode::Total, &fixture.control)
        .unwrap()
        .patch
        .contains("+new content"));
    assert_eq!(
        fixture
            .git
            .diff(&repo, &entry, DiffMode::Staged, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::Empty
    );
    assert!(fixture
        .git
        .diff(&repo, &entry, DiffMode::Unstaged, &fixture.control)
        .unwrap()
        .patch
        .contains("+new content"));
    assert!(fixture
        .git
        .contents(&repo, &entry, DiffMode::Total, &fixture.control)
        .unwrap()
        .old_file
        .is_none());
}

#[test]
fn additions_deletions_and_renames_have_correct_sides() {
    let fixture = Fixture::new();
    fixture.write("old.txt", "first\nsecond\nthird\nfourth\nfifth\nsixth\n");
    fixture.write("deleted.txt", "removed\n");
    fixture.commit();
    git(fixture.root(), &["mv", "old.txt", "renamed.txt"]);
    fixture.write(
        "renamed.txt",
        "changed\nsecond\nthird\nfourth\nfifth\nsixth\n",
    );
    fs::remove_file(fixture.root().join("deleted.txt")).unwrap();
    fixture.write("added.txt", "untracked\n");
    let repo = fixture.repository();
    let renamed = fixture.entry("renamed.txt");
    assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
    let diff = fixture
        .git
        .diff(&repo, &renamed, DiffMode::Total, &fixture.control)
        .unwrap();
    assert!(diff.patch.contains("rename from old.txt") && diff.patch.contains("+changed"));
    let contents = fixture
        .git
        .contents(&repo, &renamed, DiffMode::Total, &fixture.control)
        .unwrap();
    assert_eq!(contents.old_file.unwrap().name, "old.txt");
    assert_eq!(contents.new_file.unwrap().name, "renamed.txt");
    let deletion = fixture.entry("deleted.txt");
    let contents = fixture
        .git
        .contents(&repo, &deletion, DiffMode::Total, &fixture.control)
        .unwrap();
    assert!(contents.new_file.is_none());
    assert_eq!(contents.old_file.unwrap().contents, "removed\n");
    let addition = fixture.entry("added.txt");
    assert_eq!(
        fixture
            .git
            .diff(&repo, &addition, DiffMode::Staged, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::Empty
    );
    let added_diff = fixture
        .git
        .diff(&repo, &addition, DiffMode::Unstaged, &fixture.control)
        .unwrap();
    assert!(added_diff.patch.contains("+untracked"));
}

#[test]
fn renamed_files_remain_a_single_comparison_after_large_unstaged_edits() {
    let fixture = Fixture::new();
    fixture.write("old.txt", "first\nsecond\nthird\nfourth\nfifth\nsixth\n");
    fixture.commit();
    git(fixture.root(), &["mv", "old.txt", "new.txt"]);
    fixture.write("new.txt", "completely\ndifferent\ncontent\nnow\n");
    let entry = fixture.entry("new.txt");
    assert_eq!(entry.old_path.as_deref(), Some("old.txt"));
    let diff = fixture
        .git
        .diff(
            &fixture.repository(),
            &entry,
            DiffMode::Total,
            &fixture.control,
        )
        .unwrap();
    assert_eq!(
        diff.patch
            .lines()
            .filter(|line| line.starts_with("diff --git "))
            .count(),
        1
    );
    assert!(diff.patch.contains("--- \"a/old.txt\""));
    assert!(diff.patch.contains("+++ \"b/new.txt\""));
    assert!(diff.patch.contains("-first") && diff.patch.contains("+completely"));
}

#[test]
fn binary_large_and_filtered_files_have_explicit_limits() {
    let fixture = Fixture::new();
    fixture.write("binary.dat", [0, 1, 2, 3]);
    fixture.commit();
    fixture.write("binary.dat", [0, 1, 4, 5]);
    fixture.write("large.txt", vec![b'x'; CONTENT_LIMIT_BYTES + 1]);
    fixture.write(".gitattributes", "*.lfs filter=lfs\n");
    fixture.write("asset.lfs", "version https://git-lfs.github.com/spec/v1\n");
    let repo = fixture.repository();
    let binary = fixture.entry("binary.dat");
    assert_eq!(
        fixture
            .git
            .diff(&repo, &binary, DiffMode::Total, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::Binary
    );
    assert!(fixture
        .git
        .contents(&repo, &binary, DiffMode::Total, &fixture.control)
        .is_err());
    let large = fixture.entry("large.txt");
    assert_eq!(
        fixture
            .git
            .diff(&repo, &large, DiffMode::Total, &fixture.control)
            .unwrap()
            .kind,
        DiffKind::TooLarge
    );
    let filtered = fixture.entry("asset.lfs");
    let diff = fixture
        .git
        .diff(&repo, &filtered, DiffMode::Total, &fixture.control)
        .unwrap();
    assert_eq!(diff.kind, DiffKind::Metadata);
    assert!(diff.message.unwrap().contains("Filtered/LFS"));
}

#[test]
fn conflicts_are_identified_without_a_fake_two_way_comparison() {
    let fixture = Fixture::new();
    fixture.write("file.txt", "base\n");
    fixture.commit();
    git(fixture.root(), &["checkout", "--quiet", "-b", "other"]);
    fixture.write("file.txt", "other\n");
    fixture.commit();
    git(fixture.root(), &["checkout", "--quiet", "main"]);
    fixture.write("file.txt", "main\n");
    fixture.commit();
    let merged = Command::new("git")
        .current_dir(fixture.root())
        .args(["merge", "other"])
        .output()
        .unwrap();
    assert!(!merged.status.success());
    let entry = fixture.entry("file.txt");
    assert_eq!(entry.index_status, "U");
    assert_eq!(
        fixture
            .git
            .diff(
                &fixture.repository(),
                &entry,
                DiffMode::Total,
                &fixture.control
            )
            .unwrap()
            .kind,
        DiffKind::Conflict
    );
}

#[cfg(unix)]
#[test]
fn symlinks_are_compared_as_links_and_discovery_does_not_follow_them() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = Fixture::new();
    fs::write(outside.root().join("target.txt"), "secret target contents").unwrap();
    symlink(outside.root(), fixture.root().join("directory-link")).unwrap();
    symlink(
        outside.root().join("target.txt"),
        fixture.root().join("file-link"),
    )
    .unwrap();
    let repositories = fixture
        .git
        .discover(fixture.root(), &[], &fixture.control)
        .unwrap();
    assert_eq!(repositories.len(), 1);
    let entry = fixture.entry("file-link");
    let contents = fixture
        .git
        .contents(
            &fixture.repository(),
            &entry,
            DiffMode::Unstaged,
            &fixture.control,
        )
        .unwrap();
    let text = contents.new_file.unwrap().contents;
    assert!(text.ends_with("target.txt"));
    assert!(!text.contains("secret target contents"));
}

#[cfg(unix)]
#[test]
fn cancellation_kills_and_reaps_a_running_process() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let directory = tempdir().unwrap();
    init(directory.path());
    let repository = CliGit::new()
        .discover(directory.path(), &[], &Cancellation::new())
        .unwrap()
        .remove(0);
    let executable = directory.path().join("fake-git");
    // No shell in the production adapter. The fixture uses a Python executable
    // so its process remains alive until the adapter cancels it.
    fs::write(&executable, "#!/usr/bin/python3\nimport os, time\nopen(os.path.join(os.getcwd(), 'pid'), 'w').write(str(os.getpid()))\ntime.sleep(20)\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let backend = CliGit::with_executable(executable.as_os_str());
    let token = Cancellation::new();
    let cancel = token.clone();
    let root = directory.path().to_path_buf();
    let start = Instant::now();
    let worker = std::thread::spawn(move || backend.status(&repository, &root, &token));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !directory.path().join("pid").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    cancel.cancel();
    assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
    assert!(start.elapsed() < Duration::from_secs(4));
    let pid: i32 = fs::read_to_string(directory.path().join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}
