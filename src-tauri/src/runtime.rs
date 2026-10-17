use gitvista_core::*;
use gitvista_git::CliGit;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::ipc::Channel;
use tokio::sync::{mpsc, Semaphore};

#[derive(Clone)]
pub struct Runtime(Arc<Inner>);
struct Inner {
    data: Mutex<Data>,
    preferences: Mutex<Preferences>,
    preferences_path: PathBuf,
    backend: Arc<CliGit>,
    background: Arc<Semaphore>,
    foreground: Arc<Semaphore>,
    diff: Mutex<(u64, Cancellation, u64)>,
    cache: Mutex<ByteLru<String, DiffResult>>,
    channel: Mutex<Option<Channel<WorkspaceEvent>>>,
    editor_active: AtomicBool,
    repository_write: AtomicBool,
    badges: crate::root_badges::BadgeChecks,
}
struct Data {
    root: Option<PathBuf>,
    generation: u64,
    repositories: HashMap<String, RepoState>,
    revisions: HashMap<String, u64>,
    scanning: bool,
    scan_running: bool,
    scanned_count: usize,
    control: Cancellation,
    watcher: Option<RecommendedWatcher>,
    last_full_refresh: Option<Instant>,
    watch_error: Option<String>,
}

impl Runtime {
    pub fn new(preferences_path: PathBuf) -> Self {
        let mut preferences: Preferences = fs::read(&preferences_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // Allows opening a project from Finder/terminal and is useful for native QA.
        let args: Vec<_> = std::env::args_os().collect();
        let path = if args.get(1).is_some_and(|s| s == "--workspace") {
            args.get(2)
        } else {
            args.get(1)
                .filter(|s| !s.to_string_lossy().starts_with('-'))
        };
        if let Some(path) = path {
            if Path::new(path).is_dir() {
                preferences.last_root = Some(path.to_string_lossy().into_owned());
            }
        }
        Self(Arc::new(Inner {
            data: Mutex::new(Data {
                root: None,
                generation: 0,
                repositories: HashMap::new(),
                revisions: HashMap::new(),
                scanning: false,
                scan_running: false,
                scanned_count: 0,
                control: Cancellation::new(),
                watcher: None,
                last_full_refresh: None,
                watch_error: None,
            }),
            preferences: Mutex::new(preferences),
            preferences_path,
            backend: Arc::new(CliGit::new()),
            background: Arc::new(Semaphore::new(3)),
            foreground: Arc::new(Semaphore::new(1)),
            diff: Mutex::new((0, Cancellation::new(), 0)),
            cache: Mutex::new(ByteLru::new(CACHE_BUDGET_BYTES)),
            channel: Mutex::new(None),
            editor_active: AtomicBool::new(false),
            repository_write: AtomicBool::new(false),
            badges: crate::root_badges::BadgeChecks::new(),
        }))
    }
    pub fn preferences(&self) -> Preferences {
        self.0.preferences.lock().unwrap().clone()
    }
    pub async fn root_badges(
        &self,
        roots: Vec<String>,
        force: bool,
    ) -> Result<Vec<crate::root_badges::RootBadge>, String> {
        self.0
            .badges
            .check(
                self.0.backend.clone(),
                self.0.background.clone(),
                roots,
                self.preferences().extra_repositories,
                force,
            )
            .await
    }
    pub fn cancel_root_badges(&self) {
        self.0.badges.cancel();
    }
    pub fn editor_active(&self) -> bool {
        self.0.editor_active.load(Ordering::Acquire) || self.repository_write_active()
    }
    pub fn repository_write_active(&self) -> bool {
        self.0.repository_write.load(Ordering::Acquire)
    }
    pub fn set_editor_active(&self, active: bool) {
        self.0.editor_active.store(active, Ordering::Release);
    }
    pub fn save_preferences(&self, mut preferences: Preferences) -> Result<(), String> {
        // Serialize atomic replacements: UI saves and root transitions can overlap.
        let mut stored = self.0.preferences.lock().unwrap();
        preferences.sidebar_width = preferences.sidebar_width.clamp(220.0, 520.0);
        preferences.recent_roots.truncate(12);
        let parent = self
            .0
            .preferences_path
            .parent()
            .ok_or("Invalid preferences path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = self.0.preferences_path.with_extension("tmp");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(&preferences).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(temporary, &self.0.preferences_path).map_err(|e| e.to_string())?;
        *stored = preferences;
        Ok(())
    }
    fn send(&self, event: WorkspaceEvent) {
        if let Some(channel) = self.0.channel.lock().unwrap().as_ref() {
            let _ = channel.send(event);
        }
    }
    fn snapshot(&self) -> WorkspaceSnapshot {
        let data = self.0.data.lock().unwrap();
        let mut repositories: Vec<_> = data.repositories.values().cloned().collect();
        repositories.sort_by(|a, b| a.repository.relative_path.cmp(&b.repository.relative_path));
        WorkspaceSnapshot {
            root: data
                .root
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            generation: data.generation,
            repositories,
            scanning: data.scanning,
            scanned_count: data.scanned_count,
        }
    }
    pub async fn open(
        &self,
        root: String,
        channel: Channel<WorkspaceEvent>,
    ) -> Result<WorkspaceSnapshot, String> {
        let root =
            fs::canonicalize(root).map_err(|e| format!("No se pudo abrir la carpeta: {e}"))?;
        if !root.is_dir() {
            return Err("La raíz debe ser una carpeta".into());
        }
        {
            let mut data = self.0.data.lock().unwrap();
            if self.repository_write_active() {
                return Err("Esperá a que termine la limpieza del repositorio.".into());
            }
            data.control.cancel();
            data.watcher = None;
            data.control = Cancellation::new();
            data.generation += 1;
            data.root = Some(root.clone());
            data.repositories.clear();
            data.revisions.clear();
            data.scanned_count = 0;
            data.scanning = true;
            data.scan_running = false;
            data.last_full_refresh = None;
            data.watch_error = None;
        }
        {
            let mut active = self.0.diff.lock().unwrap();
            active.1.cancel();
            *active = (0, Cancellation::new(), 0);
        }
        self.0.cache.lock().unwrap().clear();
        *self.0.channel.lock().unwrap() = Some(channel);
        let mut preferences = self.preferences();
        let display = root.to_string_lossy().into_owned();
        preferences.recent_roots.retain(|p| p != &display);
        preferences.recent_roots.insert(0, display.clone());
        preferences.last_root = Some(display);
        self.save_preferences(preferences)?;
        let snapshot = self.snapshot();
        self.send(WorkspaceEvent::Reset {
            snapshot: snapshot.clone(),
        });
        let control = self.0.data.lock().unwrap().control.clone();
        self.install_watchers(snapshot.generation, root, control);
        self.spawn_scan(true, None);
        Ok(snapshot)
    }
    pub async fn refresh(&self, force: bool) -> Result<(), String> {
        {
            let data = self.0.data.lock().unwrap();
            if data.root.is_none() || data.scanning {
                return Ok(());
            }
            if !force
                && data
                    .last_full_refresh
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
            {
                return Ok(());
            }
        }
        self.spawn_scan(force, None);
        Ok(())
    }
    pub fn close(&self) -> Result<(), String> {
        {
            let mut data = self.0.data.lock().unwrap();
            if self.repository_write_active() {
                return Err("Esperá a que termine la limpieza del repositorio.".into());
            }
            data.control.cancel();
            data.watcher = None;
            data.generation += 1;
            data.root = None;
            data.repositories.clear();
            data.revisions.clear();
            data.scanned_count = 0;
            data.scanning = false;
            data.scan_running = false;
            data.last_full_refresh = None;
            data.watch_error = None;
        }
        {
            let mut active = self.0.diff.lock().unwrap();
            active.1.cancel();
            *active = (0, Cancellation::new(), 0);
        }
        self.0.cache.lock().unwrap().clear();
        *self.0.channel.lock().unwrap() = None;
        let mut preferences = self.preferences();
        preferences.last_root = None;
        self.save_preferences(preferences)
    }
    pub async fn add_repository(&self, path: String) -> Result<(), String> {
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        let root = self
            .0
            .data
            .lock()
            .unwrap()
            .root
            .clone()
            .ok_or("Abrí una carpeta primero")?;
        if !path.starts_with(&root) {
            return Err("El repositorio debe estar dentro de la raíz activa".into());
        }
        let backend = self.0.backend.clone();
        let check_root = path.clone();
        let check_path = path.clone();
        let control = Cancellation::new();
        let permit = self
            .0
            .background
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let found = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            backend.discover(&check_root, &[check_path], &control)
        })
        .await
        .map_err(|e| e.to_string())??;
        if !found.iter().any(|repo| repo.directory() == path) {
            return Err("La carpeta seleccionada no es la raíz de un repositorio Git".into());
        }
        if self.0.data.lock().unwrap().root.as_ref() != Some(&root) {
            return Err("Operation cancelled".into());
        }
        let mut preferences = self.preferences();
        let paths = preferences
            .extra_repositories
            .entry(root.to_string_lossy().into_owned())
            .or_default();
        let display = path.to_string_lossy().into_owned();
        if !paths.contains(&display) {
            paths.push(display);
        }
        self.save_preferences(preferences)?;
        while self.0.data.lock().unwrap().scanning {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.spawn_scan(true, None);
        Ok(())
    }
    fn spawn_scan(&self, rediscover: bool, affected: Option<HashSet<String>>) {
        let (root, generation, control) = {
            let mut data = self.0.data.lock().unwrap();
            if self.repository_write_active() {
                return;
            }
            let Some(root) = data.root.clone() else {
                return;
            };
            if data.scan_running {
                return;
            }
            data.scan_running = true;
            data.scanning = true;
            data.scanned_count = 0;
            (root, data.generation, data.control.clone())
        };
        self.send(WorkspaceEvent::Reset {
            snapshot: self.snapshot(),
        });
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let start = Instant::now();
            let mut repositories = if rediscover {
                let backend = runtime.0.backend.clone();
                let extras = runtime
                    .preferences()
                    .extra_repositories
                    .get(&root.to_string_lossy().into_owned())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();
                let discover_root = root.clone();
                let token = control.clone();
                // Discovery shares the background budget with statuses from the previous root.
                let permit = runtime.0.background.clone().acquire_owned().await.unwrap();
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    backend.discover(&discover_root, &extras, &token)
                })
                .await;
                match result {
                    Ok(Ok(repositories)) => repositories,
                    other => {
                        let message = match other {
                            Ok(Err(e)) => e,
                            Err(e) => e.to_string(),
                            _ => unreachable!(),
                        };
                        runtime.finish_error(generation, &control, message);
                        return;
                    }
                }
            } else {
                runtime
                    .0
                    .data
                    .lock()
                    .unwrap()
                    .repositories
                    .values()
                    .map(|r| r.repository.clone())
                    .collect()
            };
            if control.is_cancelled() {
                return;
            }
            {
                let mut data = runtime.0.data.lock().unwrap();
                if data.generation != generation {
                    return;
                }
                if rediscover {
                    let ids: HashSet<_> = repositories.iter().map(|r| r.id.clone()).collect();
                    data.repositories.retain(|id, _| ids.contains(id));
                    data.revisions.retain(|id, _| ids.contains(id));
                    for repository in &repositories {
                        data.repositories
                            .entry(repository.id.clone())
                            .or_insert_with(|| RepoState {
                                repository: repository.clone(),
                                entries: vec![],
                                error: None,
                                elapsed_ms: 0,
                            });
                    }
                }
            }
            runtime.send(WorkspaceEvent::Reset {
                snapshot: runtime.snapshot(),
            });
            if let Some(ids) = affected {
                repositories.retain(|r| ids.contains(&r.id));
            }
            repositories.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
            let total = repositories.len();
            let mut tasks = tokio::task::JoinSet::new();
            for repository in repositories {
                if control.is_cancelled() {
                    break;
                }
                let backend = runtime.0.backend.clone();
                let status_root = root.clone();
                let token = control.clone();
                let semaphore = runtime.0.background.clone();
                tasks.spawn(async move {
                    let permit = semaphore.acquire_owned().await.unwrap();
                    let started = Instant::now();
                    let repo = repository.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let _permit = permit;
                        backend.status(&repo, &status_root, &token)
                    })
                    .await;
                    let result = result.unwrap_or_else(|e| Err(e.to_string()));
                    (repository, result, started.elapsed().as_millis() as u64)
                });
            }
            while let Some(result) = tasks.join_next().await {
                let Ok((repository, entries, elapsed_ms)) = result else {
                    continue;
                };
                if control.is_cancelled() {
                    return;
                }
                let state = {
                    let mut data = runtime.0.data.lock().unwrap();
                    if data.generation != generation {
                        return;
                    }
                    let (entries, error) = match entries {
                        Ok(entries) => (entries, None),
                        Err(error) => (
                            data.repositories
                                .get(&repository.id)
                                .map(|r| r.entries.clone())
                                .unwrap_or_default(),
                            Some(error),
                        ),
                    };
                    let state = RepoState {
                        repository: repository.clone(),
                        entries,
                        error,
                        elapsed_ms,
                    };
                    let revision = data.revisions.entry(repository.id.clone()).or_default();
                    *revision += 1;
                    data.repositories.insert(repository.id, state.clone());
                    data.scanned_count += 1;
                    (state, data.scanned_count)
                };
                runtime.send(WorkspaceEvent::Repository {
                    generation,
                    state: state.0,
                    scanned_count: state.1,
                    total_count: total,
                });
            }
            {
                let mut data = runtime.0.data.lock().unwrap();
                if data.generation != generation || control.is_cancelled() {
                    return;
                }
                data.scanning = false;
                data.scan_running = false;
                if rediscover {
                    data.last_full_refresh = Some(Instant::now());
                }
            }
            runtime.send(WorkspaceEvent::Done {
                generation,
                elapsed_ms: start.elapsed().as_millis() as u64,
            });
            if rediscover {
                runtime.watch_external_git_dirs(generation, &root);
            }
            let watch_error = runtime.0.data.lock().unwrap().watch_error.clone();
            if let Some(message) = watch_error {
                runtime.send(WorkspaceEvent::Error {
                    generation,
                    message,
                });
            }
        });
    }
    fn finish_error(&self, generation: u64, control: &Cancellation, message: String) {
        if control.is_cancelled() {
            return;
        }
        let mut data = self.0.data.lock().unwrap();
        if data.generation != generation {
            return;
        }
        data.scanning = false;
        data.scan_running = false;
        drop(data);
        self.send(WorkspaceEvent::Error {
            generation,
            message,
        });
    }
    fn install_watchers(&self, generation: u64, root: PathBuf, control: Cancellation) {
        let (sender, mut receiver) = mpsc::channel::<notify::Result<Event>>(128);
        let overflow = Arc::new(AtomicBool::new(false));
        let callback_overflow = overflow.clone();
        let watcher = notify::recommended_watcher(move |event| {
            if sender.try_send(event).is_err() {
                callback_overflow.store(true, Ordering::Release);
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                self.0.data.lock().unwrap().watch_error = Some(format!(
                    "La actualización automática no está disponible: {e}"
                ));
                return;
            }
        };
        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            self.0.data.lock().unwrap().watch_error = Some(format!(
                "La actualización automática no está disponible: {e}"
            ));
            return;
        }
        {
            let mut data = self.0.data.lock().unwrap();
            if data.generation != generation {
                return;
            }
            data.watcher = Some(watcher);
        }
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(first) = receiver.recv().await {
                if control.is_cancelled() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
                let mut events = vec![first];
                while let Ok(event) = receiver.try_recv() {
                    events.push(event);
                }
                let mut rescan = overflow.swap(false, Ordering::AcqRel);
                let mut dirty = HashSet::new();
                {
                    let data = runtime.0.data.lock().unwrap();
                    if data.generation != generation {
                        break;
                    }
                    let registered: Vec<_> = data
                        .repositories
                        .values()
                        .map(|state| state.repository.directory().to_path_buf())
                        .collect();
                    for event in events {
                        match event {
                            Err(_) => rescan = true,
                            Ok(event) => {
                                if event.need_rescan() {
                                    rescan = true;
                                }
                                for path in event.paths {
                                    if ignored_event(&path, &root, &registered) {
                                        continue;
                                    }
                                    if path.file_name().is_some_and(|n| n == ".git") {
                                        rescan = true;
                                    }
                                    let known_git_dir = data.repositories.values().any(|state| {
                                        path.starts_with(state.repository.git_directory())
                                    });
                                    if !known_git_dir
                                        && path.components().any(|part| part.as_os_str() == ".git")
                                    {
                                        rescan = true;
                                    }
                                    if path.file_name().is_some_and(|name| {
                                        name == "HEAD" || name == "config" || name == "commondir"
                                    }) && data.repositories.values().any(|state| {
                                        path.starts_with(state.repository.git_directory())
                                            || path.starts_with(state.repository.common_directory())
                                    }) {
                                        rescan = true;
                                    }
                                    for state in data.repositories.values() {
                                        let repo = &state.repository;
                                        if path.starts_with(repo.git_directory())
                                            || path.starts_with(repo.common_directory())
                                        {
                                            dirty.insert(repo.id.clone());
                                        }
                                        // Parents may track a gitlink or the same path as a nested repo.
                                        if path.starts_with(repo.directory()) {
                                            dirty.insert(repo.id.clone());
                                        }
                                    }
                                    if dirty.is_empty() && path.is_dir() {
                                        rescan = true;
                                    }
                                }
                            }
                        }
                    }
                }
                if rescan || !dirty.is_empty() {
                    // Do not lose invalidations that arrive while an earlier refresh is running.
                    loop {
                        if control.is_cancelled() {
                            return;
                        }
                        if !runtime.0.data.lock().unwrap().scanning {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    runtime.spawn_scan(rescan, if rescan { None } else { Some(dirty) });
                }
            }
        });
    }
    fn watch_external_git_dirs(&self, generation: u64, root: &Path) {
        let mut data = self.0.data.lock().unwrap();
        if data.generation != generation {
            return;
        }
        let external: HashSet<_> = data
            .repositories
            .values()
            .flat_map(|state| {
                [
                    state.repository.git_directory().to_path_buf(),
                    state.repository.common_directory().to_path_buf(),
                ]
            })
            .filter(|path| !path.starts_with(root))
            .collect();
        let mut watch_error = None;
        if let Some(watcher) = data.watcher.as_mut() {
            for path in external {
                if let Err(error) = watcher.watch(&path, RecursiveMode::Recursive) {
                    watch_error = Some(format!("No se pudo observar {}: {error}", path.display()));
                }
            }
        }
        if watch_error.is_some() {
            data.watch_error = watch_error;
        }
    }
    fn selected(&self, entry_id: &str) -> Result<(Repository, ChangeEntry, u64, u64), String> {
        let data = self.0.data.lock().unwrap();
        for state in data.repositories.values() {
            if let Some(entry) = state.entries.iter().find(|e| e.id == entry_id) {
                return Ok((
                    state.repository.clone(),
                    entry.clone(),
                    data.generation,
                    *data.revisions.get(&state.repository.id).unwrap_or(&0),
                ));
            }
        }
        Err("El archivo ya no está en la lista de cambios; actualizá el proyecto".into())
    }
    pub async fn diff(
        &self,
        entry_id: String,
        mode: DiffMode,
        request_id: u64,
    ) -> Result<DiffResponse, String> {
        let (repo, entry, generation, revision) = self.selected(&entry_id)?;
        let control = {
            let mut active = self.0.diff.lock().unwrap();
            if request_id < active.0 {
                return Err("Operation cancelled".into());
            }
            active.1.cancel();
            *active = (request_id, Cancellation::new(), revision);
            active.1.clone()
        };
        let key = format!("{generation}:{revision}:{entry_id}:{mode:?}");
        if let Some(result) = self.0.cache.lock().unwrap().get(&key).cloned() {
            return Ok(DiffResponse {
                request_id,
                generation,
                result,
            });
        }
        let backend = self.0.backend.clone();
        let permit = self
            .0
            .foreground
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        control.check()?;
        let token = control.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            backend.diff(&repo, &entry, mode, &token)
        })
        .await
        .map_err(|e| e.to_string())??;
        control.check()?;
        if self.0.data.lock().unwrap().generation != generation {
            return Err("Operation cancelled".into());
        }
        if self.selected(&entry_id)?.3 != revision {
            return Err("El archivo cambió durante la consulta. Actualizá el diff.".into());
        }
        self.0
            .cache
            .lock()
            .unwrap()
            .insert(key, result.clone(), result.patch.len() + 512);
        Ok(DiffResponse {
            request_id,
            generation,
            result,
        })
    }
    pub async fn contents(
        &self,
        entry_id: String,
        mode: DiffMode,
        request_id: u64,
    ) -> Result<DiffContents, String> {
        let (repo, entry, generation, revision) = self.selected(&entry_id)?;
        let control = {
            let active = self.0.diff.lock().unwrap();
            if active.0 != request_id || active.2 != revision {
                return Err("El archivo cambió desde la comparación. Actualizá el diff.".into());
            }
            active.1.clone()
        };
        let permit = self
            .0
            .foreground
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        control.check()?;
        let backend = self.0.backend.clone();
        let token = control.clone();
        let contents = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut contents = backend.contents(&repo, &entry, mode, &token)?;
            if mode == DiffMode::Staged {
                contents.edit_unavailable_reason = Some("Preparado muestra el índice. Usá Total o Sin preparar para editar el archivo actual.".into());
            } else if let Some(file) = &contents.new_file {
                match crate::editor::inspect(&repo, &entry, &file.contents) {
                    Ok(token) => contents.edit_version = Some(EditVersion { generation, token }),
                    Err(reason) => contents.edit_unavailable_reason = Some(reason),
                }
            } else {
                contents.edit_unavailable_reason = Some("La versión nueva no existe en disco.".into());
            }
            Ok::<_, String>(contents)
        })
        .await
        .map_err(|e| e.to_string())??;
        control.check()?;
        if self.0.data.lock().unwrap().generation != generation {
            return Err("Operation cancelled".into());
        }
        if self.selected(&entry_id)?.3 != revision {
            return Err("El archivo cambió durante la lectura. Actualizá el diff.".into());
        }
        Ok(contents)
    }
    pub async fn save_file(
        &self,
        entry_id: String,
        version: EditVersion,
        contents: String,
    ) -> Result<(), String> {
        if contents.len() > CONTENT_LIMIT_BYTES {
            return Err("El archivo supera el límite de 10 MiB.".into());
        }
        let permit = self
            .0
            .foreground
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let runtime = self.clone();
        let repo_id = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            // Hold workspace ownership through the explicit write, so switching roots
            // cannot redirect a pending save. Filesystem version checks remain independent
            // of status revisions: watcher refreshes must not invalidate an unchanged draft.
            let data = runtime.0.data.lock().unwrap();
            if runtime.repository_write_active() {
                return Err("Esperá a que termine la limpieza del repositorio.".into());
            }
            if data.generation != version.generation {
                return Err("La raíz activa cambió; el borrador no se guardó.".into());
            }
            for state in data.repositories.values() {
                if let Some(entry) = state.entries.iter().find(|entry| entry.id == entry_id) {
                    crate::editor::save(&state.repository, entry, &version.token, &contents)?;
                    return Ok(state.repository.id.clone());
                }
            }
            Err::<String, String>(
                "El archivo ya no está en la lista de cambios. Tu borrador se conserva.".into(),
            )
        })
        .await
        .map_err(|e| e.to_string())??;
        self.0.cache.lock().unwrap().clear();
        self.spawn_scan(false, Some(HashSet::from([repo_id])));
        Ok(())
    }

    pub async fn stash_preview(
        &self,
        repo_id: String,
        generation: u64,
    ) -> Result<StashPreview, String> {
        let repo = self.repository_for_write(&repo_id, generation)?;
        let control = self.0.data.lock().unwrap().control.clone();
        let backend = self.0.backend.clone();
        let permit = self
            .0
            .foreground
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let preview = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            backend.stash_preview(&repo, &control)
        })
        .await
        .map_err(|e| e.to_string())??;
        self.repository_for_write(&repo_id, generation)?;
        Ok(preview)
    }

    fn repository_for_write(&self, repo_id: &str, generation: u64) -> Result<Repository, String> {
        let data = self.0.data.lock().unwrap();
        if data.generation != generation {
            return Err("La raíz activa cambió. Volvé a seleccionar el repositorio.".into());
        }
        let state = data
            .repositories
            .get(repo_id)
            .ok_or("El repositorio ya no está disponible.")?;
        if state.error.is_some() {
            return Err("Actualizá el repositorio y resolvé el error antes de limpiarlo.".into());
        }
        Ok(state.repository.clone())
    }

    pub async fn stash_repository(
        &self,
        repo_id: String,
        generation: u64,
        token: String,
        include_untracked: bool,
    ) -> Result<StashResult, String> {
        let permit = self
            .0
            .foreground
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let repo = self.repository_for_write(&repo_id, generation)?;
        {
            let data = self.0.data.lock().unwrap();
            if data.generation != generation {
                return Err("La raíz activa cambió.".into());
            }
            if data.scan_running {
                return Err(
                    "Esperá a que termine la consulta de cambios y volvé a intentar.".into(),
                );
            }
            if self.editor_active() {
                return Err("Terminá de editar antes de limpiar el repositorio.".into());
            }
            self.0.repository_write.store(true, Ordering::Release);
        }
        // Ownership travels into the blocking task: abandoning the IPC future must
        // not release the close/root-switch guard while Git is still writing.
        let guard = RepositoryWriteGuard(self.clone());
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = runtime
                .0
                .backend
                .stash_repository(&repo, &token, include_untracked);
            runtime.0.cache.lock().unwrap().clear();
            drop(guard);
            runtime.spawn_scan(false, Some(HashSet::from([repo_id])));
            result
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

struct RepositoryWriteGuard(Runtime);
impl Drop for RepositoryWriteGuard {
    fn drop(&mut self) {
        (self.0).0.repository_write.store(false, Ordering::Release);
    }
}

fn ignored_event(path: &Path, root: &Path, registered: &[PathBuf]) -> bool {
    let containing = registered
        .iter()
        .filter(|repo| path.starts_with(repo))
        .max_by_key(|repo| repo.components().count())
        .map(PathBuf::as_path)
        .unwrap_or(root);
    let relative = path.strip_prefix(containing).unwrap_or(path);
    // Object/log churn is irrelevant; index/HEAD/refs/attributes must reach Git.
    let parts: Vec<_> = relative.components().map(|c| c.as_os_str()).collect();
    if parts.iter().any(|p| {
        ["node_modules", "target", "dist", ".venv", ".cache"]
            .iter()
            .any(|name| *p == *name)
    }) {
        return true;
    }
    if let Some(index) = parts.iter().position(|p| *p == ".git") {
        if parts
            .get(index + 1)
            .is_some_and(|p| ["objects", "logs"].iter().any(|name| *p == *name))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn repository_write_guard_blocks_root_changes_and_survives_editor_flag_reset() {
        let fixture = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(fixture.path().join("preferences.json"));
        runtime.0.repository_write.store(true, Ordering::Release);
        let guard = RepositoryWriteGuard(runtime.clone());
        runtime.set_editor_active(false);
        assert!(
            runtime.editor_active(),
            "Native close/quit must stay guarded"
        );
        assert!(runtime.close().is_err());
        let channel = Channel::new(|_| Ok(()));
        assert!(runtime
            .open(fixture.path().to_string_lossy().into(), channel)
            .await
            .is_err());
        runtime.spawn_scan(false, None);
        assert!(!runtime.0.data.lock().unwrap().scan_running);
        drop(guard);
        assert!(!runtime.editor_active());
        assert!(runtime.close().is_ok());
    }

    #[test]
    fn watcher_keeps_git_state_and_skips_generated_churn() {
        let root = Path::new("/work");
        assert!(!ignored_event(
            Path::new("/work/repo/.git/index"),
            root,
            &[]
        ));
        assert!(!ignored_event(
            Path::new("/work/repo/.git/refs/heads/main"),
            root,
            &[]
        ));
        assert!(ignored_event(
            Path::new("/work/repo/.git/objects/aa/123"),
            root,
            &[]
        ));
        assert!(ignored_event(
            Path::new("/work/node_modules/pkg/file"),
            root,
            &[]
        ));
        let registered = vec![PathBuf::from("/work/node_modules/custom")];
        assert!(!ignored_event(
            Path::new("/work/node_modules/custom/src/file.rs"),
            root,
            &registered
        ));
        assert!(ignored_event(
            Path::new("/work/node_modules/custom/node_modules/dep/file"),
            root,
            &registered
        ));
    }
}
