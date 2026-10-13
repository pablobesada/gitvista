//! Dormant roots retain only a small badge result; no watchers, snapshots or diffs.
use gitvista_core::Cancellation;
use gitvista_git::CliGit;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const BADGE_TTL: Duration = Duration::from_secs(60);
const ROOT_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootBadge {
    pub root: String,
    pub has_changes: Option<bool>,
    pub error: Option<String>,
}

#[derive(Default)]
struct State {
    control: Cancellation,
    revision: u64,
    cache: HashMap<String, (Instant, RootBadge)>,
}

pub struct BadgeChecks {
    state: Mutex<State>,
    serial: Arc<Semaphore>,
}

impl BadgeChecks {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            serial: Arc::new(Semaphore::new(1)),
        }
    }
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap();
        state.control.cancel();
        state.revision += 1;
    }

    pub async fn check(
        &self,
        backend: Arc<CliGit>,
        background: Arc<Semaphore>,
        roots: Vec<String>,
        extras: HashMap<String, Vec<String>>,
        force: bool,
    ) -> Result<Vec<RootBadge>, String> {
        let mut seen = HashSet::new();
        let roots: Vec<_> = roots
            .into_iter()
            .filter(|root| seen.insert(root.clone()))
            .collect();
        let (control, revision) = {
            let mut state = self.state.lock().unwrap();
            state.control.cancel();
            state.control = Cancellation::new();
            state.revision += 1;
            state.cache.retain(|root, _| roots.contains(root));
            (state.control.clone(), state.revision)
        };
        let mut badges = Vec::with_capacity(roots.len());
        for root in roots {
            control.check()?;
            let cached = self
                .state
                .lock()
                .unwrap()
                .cache
                .get(&root)
                .filter(|(time, _)| !force && time.elapsed() < BADGE_TTL)
                .map(|(_, badge)| badge.clone());
            if let Some(badge) = cached {
                badges.push(badge);
                continue;
            }
            // One badge probe at a time, also within the global three-job budget.
            let serial = self
                .serial
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| e.to_string())?;
            let permit = background
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| e.to_string())?;
            control.check()?;
            let paths = extras
                .get(&root)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let path = PathBuf::from(&root);
            let git = backend.clone();
            let token = control.clone();
            let task = tokio::task::spawn_blocking(move || {
                let _serial = serial;
                let _permit = permit;
                // The common dirty-root case needs just one Git command, no tree walk.
                let root_checked = path.join(".git").exists();
                if root_checked && git.has_changes(&path, &path, &token)? {
                    return Ok(true);
                }
                for repo in git.discover(&path, &paths, &token)? {
                    token.check()?;
                    if root_checked
                        && repo.directory()
                            == path.canonicalize().map_err(|error| error.to_string())?
                    {
                        continue;
                    }
                    if git.has_changes(repo.directory(), &path, &token)? {
                        return Ok(true);
                    }
                }
                token.check()?;
                Ok::<_, String>(false)
            });
            let result = match tokio::time::timeout(ROOT_DEADLINE, task).await {
                Ok(joined) => joined.unwrap_or_else(|error| Err(error.to_string())),
                Err(_) => {
                    control.cancel();
                    Err("La revisión del root excedió el límite de 5 segundos".into())
                }
            };
            let badge = match result {
                Ok(changed) => RootBadge {
                    root: root.clone(),
                    has_changes: Some(changed),
                    error: None,
                },
                Err(error) => RootBadge {
                    root: root.clone(),
                    has_changes: None,
                    error: Some(error),
                },
            };
            let mut state = self.state.lock().unwrap();
            if state.revision != revision {
                return Err("Operation cancelled".into());
            }
            state.cache.insert(root, (Instant::now(), badge.clone()));
            badges.push(badge);
            if control.is_cancelled() {
                break;
            }
        }
        Ok(badges)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn blur_cancels_the_process_and_releases_both_job_budgets() {
        let fixture = tempfile::tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(fixture.path())
            .status()
            .unwrap()
            .success());
        let marker = fixture.path().join("started");
        let executable = fixture.path().join("fake-git");
        fs::write(&executable, format!("#!/usr/bin/python3\nimport sys, time\nif 'status' in sys.argv:\n    open({:?}, 'w').close()\n    time.sleep(10)\n", marker.to_str().unwrap())).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let checks = Arc::new(BadgeChecks::new());
        let background = Arc::new(Semaphore::new(3));
        let checking = checks.clone();
        let budget = background.clone();
        let root = fixture.path().to_string_lossy().into_owned();
        let task = tokio::spawn(async move {
            checking
                .check(
                    Arc::new(CliGit::with_executable(executable)),
                    budget,
                    vec![root],
                    HashMap::new(),
                    false,
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let start = Instant::now();
        checks.cancel();
        assert!(task.await.unwrap().is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(background.available_permits(), 3);
        assert_eq!(checks.serial.available_permits(), 1);
        assert!(checks.state.lock().unwrap().cache.is_empty());
    }
}
