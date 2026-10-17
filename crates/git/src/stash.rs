//! Explicit, confirmed write boundary. Query paths never call this module's mutator.
use super::*;
use gitvista_core::{StashPreview, StashResult};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

impl CliGit {
    pub fn stash_preview(
        &self,
        repo: &Repository,
        control: &Cancellation,
    ) -> Result<StashPreview, String> {
        control.check()?;
        let current = self.inspect_repository(repo.directory(), repo.directory(), control)?;
        if current.directory() != repo.directory()
            || current.git_directory() != repo.git_directory()
            || current.common_directory() != repo.common_directory()
        {
            return Err("La ubicación del repositorio cambió. Volvé a abrir la raíz.".into());
        }
        if !self.has_head(repo, control)? {
            return Err(
                "Git stash necesita al menos un commit. No se modificó el repositorio.".into(),
            );
        }
        for name in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "BISECT_LOG",
        ] {
            if repo.git_directory().join(name).exists() {
                return Err(
                    "Terminá la operación Git en curso antes de limpiar este repositorio.".into(),
                );
            }
        }
        // Disabling a clean/smudge driver during a stash could corrupt its meaning;
        // executing arbitrary drivers would exceed this explicit write boundary.
        if !self.filter_overrides(repo, control)?.is_empty() {
            return Err("La limpieza todavía no admite repositorios con filtros Git/LFS configurados. Usá Git para guardar esos cambios.".into());
        }
        let entries = self.status(repo, repo.directory(), control)?;
        let mut preview = StashPreview {
            token: String::new(),
            tracked: 0,
            staged: 0,
            unstaged: 0,
            untracked: 0,
        };
        let mut hash = Sha256::new();
        hash.update(native_bytes(repo.directory().as_os_str()));
        hash.update(self.checked(
            repo.directory(),
            args(&["rev-parse", "HEAD"]),
            QUERY_LIMIT,
            control,
        )?);
        // Hash the index as a stream; never load arbitrarily large indexes in memory.
        if let Ok(mut index) = fs::File::open(repo.git_directory().join("index")) {
            let mut chunk = [0u8; 16384];
            loop {
                let read = index.read(&mut chunk).map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                hash.update(&chunk[..read]);
            }
        }
        for entry in entries {
            if entry.index_status == "U"
                || entry.worktree_status == "U"
                || matches!(
                    (entry.index_status.as_str(), entry.worktree_status.as_str()),
                    ("A", "A") | ("D", "D")
                )
            {
                return Err("Hay conflictos sin resolver. No se modificó el repositorio.".into());
            }
            // A parent's untracked repository row belongs to another worktree.
            if entry.kind != EntryKind::File {
                continue;
            }
            if entry
                .file_path()
                .ancestors()
                .skip(1)
                .filter(|path| !path.as_os_str().is_empty())
                .any(|path| repo.directory().join(path).join(".git").exists())
            {
                return Err("Este repo versiona archivos dentro de otro repositorio. Limpiá esos repositorios por separado desde Git.".into());
            }
            if entry.index_status == "?" {
                preview.untracked += 1;
            } else {
                preview.tracked += 1;
                if entry.index_status != "." {
                    preview.staged += 1;
                }
                if entry.worktree_status != "." {
                    preview.unstaged += 1;
                }
            }
            hash.update(native_bytes(entry.file_path().as_os_str()));
            hash.update([0]);
            hash.update(entry.index_status.as_bytes());
            hash.update(entry.worktree_status.as_bytes());
            // An unchanged status code can still hide an external edit.
            hash.update(format!(
                "{:?}",
                file_stamp(&repo.directory().join(entry.file_path()))
            ));
        }
        preview.token = format!("{:x}", hash.finalize());
        Ok(preview)
    }

    /// A fresh, private cancellation token prevents navigation from interrupting a
    /// write halfway through. Deadline failures are reported with any retained stash.
    pub fn stash_repository(
        &self,
        repo: &Repository,
        expected: &str,
        include_untracked: bool,
    ) -> Result<StashResult, String> {
        let control = Cancellation::new();
        let preview = self.stash_preview(repo, &control)?;
        if preview.token != expected {
            return Err("El repositorio cambió desde la confirmación. Cerrá el diálogo y volvé a intentarlo.".into());
        }
        if preview.tracked == 0 && (!include_untracked || preview.untracked == 0) {
            return Err(
                "No hay cambios propios de este repositorio para guardar con estas opciones."
                    .into(),
            );
        }
        let before = self.stash_oid(repo, &control)?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let label = format!("GitVista · limpieza {timestamp}");
        let mut arguments = args(&[
            "--no-literal-pathspecs",
            "-c",
            "submodule.recurse=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "commit.gpgsign=false",
            "stash",
            "push",
            "--message",
            &label,
        ]);
        if include_untracked {
            arguments.push("--include-untracked".into());
        }
        // Git stash internally constructs magic pathspecs for `clean`. Inheriting
        // --literal-pathspecs makes Git report success while leaving untracked
        // files behind. This command accepts no user pathspecs at all.
        // Never --all, clean -ff, recursive submodule reset or a reset fallback.
        let output = execute(
            &self.executable,
            repo.directory(),
            &arguments,
            QUERY_LIMIT,
            Duration::from_secs(120),
            &control,
        );
        let after = self.stash_oid(repo, &control);
        let mut oid = after
            .as_ref()
            .ok()
            .and_then(|oid| oid.clone())
            .filter(|oid| Some(oid) != before.as_ref());
        let mut warnings = Vec::new();
        if let Some(candidate) = &oid {
            // Another Git client can write refs/stash concurrently. Never label
            // its backup as ours merely because the ref changed.
            let subject = self.checked(
                repo.directory(),
                vec![
                    "log".into(),
                    "-1".into(),
                    "--format=%s".into(),
                    candidate.into(),
                ],
                QUERY_LIMIT,
                &control,
            );
            if !subject.is_ok_and(|bytes| String::from_utf8_lossy(&bytes).trim().ends_with(&label))
            {
                oid = None;
            }
        }
        match output {
            Ok(output) if output.status.success() => (),
            Ok(output) => warnings.push(output.error_message()),
            Err(error) => warnings.push(error.to_string()),
        }
        if let Err(error) = after {
            warnings.push(format!("No se pudo verificar el respaldo: {error}"));
        }
        if oid.is_none() {
            warnings.push("No se confirmó un stash nuevo. Revisá el estado y git stash list antes de reintentar.".into());
        }
        let remaining = match self.status(repo, repo.directory(), &control) {
            Ok(entries) => Some(
                entries
                    .iter()
                    .filter(|entry| {
                        entry.kind == EntryKind::File
                            && (include_untracked || entry.index_status != "?")
                    })
                    .count(),
            ),
            Err(error) => {
                warnings.push(format!("No se pudo verificar el estado final: {error}"));
                None
            }
        };
        if remaining.is_some_and(|count| count > 0) {
            warnings.push("Quedan cambios en el repositorio. El respaldo se conserva; no se intentó una limpieza adicional.".into());
        }
        Ok(StashResult {
            stash_oid: oid,
            remaining,
            warning: (!warnings.is_empty()).then(|| warnings.join("\n")),
        })
    }

    fn stash_oid(
        &self,
        repo: &Repository,
        control: &Cancellation,
    ) -> Result<Option<String>, String> {
        let output = self
            .run(
                repo.directory(),
                args(&["rev-parse", "--verify", "--quiet", "refs/stash"]),
                QUERY_LIMIT,
                control,
            )
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).trim().into()))
        } else if output.status.code() == Some(1) {
            Ok(None)
        } else {
            Err(output.error_message())
        }
    }
}
