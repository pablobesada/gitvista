<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import ChangeTree from './components/ChangeTree.svelte';
  import DiffView from './components/DiffView.svelte';
  import Icon from './components/Icon.svelte';
  import RepoUntrackedToggle from './components/RepoUntrackedToggle.svelte';
  import RootTabs from './components/RootTabs.svelte';
  import { addRepository, chooseRepository, chooseRoot, closeWorkspace, defaultPreferences, exitApplication, getDiff, getPreferences, getRootBadges, isDesktop, loadDiffContents, onExitRequested, openWorkspace, refreshWorkspace, savePreferences } from './lib/bridge';
  import { canonicalizeTab, closeTab, restoreRoots, workspaceBadge } from './lib/workspace-tabs';
  import { allEntries, buildChangeTree, directoryPaths, hiddenUntrackedCount, isUntracked } from './lib/tree';
  import type { ChangeEntry, DiffContents, DiffMode, DiffResponse, Preferences, RootBadgeState, TreeView, WorkspaceEvent, WorkspaceSnapshot } from './lib/types';

  let preferences = $state<Preferences>(structuredClone(defaultPreferences));
  let snapshot = $state.raw<WorkspaceSnapshot | null>(null);
  let rootBadges = $state.raw<Record<string, RootBadgeState>>({});
  let badgeRequest = 0;
  let windowActive = $state(true);
  let mode = $state<DiffMode>('total');
  let query = $state('');
  let selected = $state.raw<ChangeEntry | null>(null);
  let response = $state.raw<DiffResponse | null>(null);
  let loadingDiff = $state(false);
  let diffError = $state<string | null>(null);
  let workspaceError = $state<string | null>(null);
  let totalCount = $state(0);
  let elapsedMs = $state<number | null>(null);
  let expanded = $state(new Set<string>());
  let recentMenu = $state(false);
  let toast = $state<string | null>(null);
  let opening = $state(false);
  let resizing = $state(false);
  let editorActive = $state(false);
  let searchInput = $state<HTMLInputElement>();
  let diffView = $state<{ nextHunk: (direction: 1 | -1) => void; copySelection: () => Promise<boolean>; canLeave: () => Promise<boolean> }>();
  let changeTree = $state<{ focusSelected: () => void }>();
  let requestId = 0;
  let readingContents = false;
  let deferredDiff = false;
  let sessionId = 0;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let diffTimer: ReturnType<typeof setTimeout> | undefined;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  const states = $derived(snapshot?.repositories ?? []);
  const activeRootPath = $derived(snapshot?.root ?? null);
  const treeOptions = $derived({ view: preferences.treeView, untrackedByRepo: preferences.untrackedByRepo });
  const modeTree = $derived(buildChangeTree(states, mode, '', treeOptions));
  const tree = $derived(query.trim() ? buildChangeTree(states, mode, query, treeOptions) : modeTree);
  const visibleEntries = $derived(allEntries(tree));
  const modeCount = $derived(allEntries(modeTree).length);
  const hiddenCount = $derived(hiddenUntrackedCount(states, mode, preferences.untrackedByRepo));
  const staleCount = $derived(states.filter(state => !!state.error).length);
  const rootName = $derived(snapshot?.root.split('/').filter(Boolean).pop() ?? 'Proyecto');
  const currentRepo = $derived(states.find(state => state.repository.id === selected?.repoId)?.repository);
  const rootRepo = $derived(states.find(state => ['.', ''].includes(state.repository.relativePath))?.repository);
  const pathParts = $derived(selected?.relativePath.split('/') ?? []);
  const modes: { value: DiffMode; label: string; title: string }[] = [
    { value: 'total', label: 'Total', title: 'HEAD → archivo actual' },
    { value: 'unstaged', label: 'Sin preparar', title: 'Índice → archivo actual' },
    { value: 'staged', label: 'Preparado', title: 'HEAD → índice' },
  ];

  function showToast(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => toast = null, 2800);
  }
  function persist() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void savePreferences($state.snapshot(preferences)).catch(error => showToast(`No se pudieron guardar las preferencias: ${String(error)}`));
    }, 450);
  }
  function expansionKey(root: string, view = preferences.treeView) {
    return view === 'folders' ? root : `${root}\0repositories`;
  }
  async function setTreeView(view: TreeView) {
    if (!await leaveEditor()) return;
    preferences.treeView = view;
    expanded = new Set(preferences.expandedPaths[expansionKey(snapshot?.root ?? '', view)] ?? []);
    persist();
  }
  async function setUntrackedVisibility(repoId: string, show: boolean) {
    if (!show && selected?.repoId === repoId && isUntracked(selected) && !await leaveEditor()) return;
    preferences.untrackedByRepo = { ...preferences.untrackedByRepo, [repoId]: show };
    if (!show && selected?.repoId === repoId && isUntracked(selected)) {
      ++requestId;
      clearTimeout(diffTimer);
      selected = null;
      response = null;
      diffError = null;
      loadingDiff = false;
    }
    persist();
  }
  function togglePath(path: string) {
    const next = new Set(expanded);
    if (next.has(path)) next.delete(path); else next.add(path);
    expanded = next;
    if (snapshot) preferences.expandedPaths = { ...preferences.expandedPaths, [expansionKey(snapshot.root)]: [...next] };
    persist();
  }
  function collapseAll() {
    expanded = new Set();
    if (snapshot) preferences.expandedPaths = { ...preferences.expandedPaths, [expansionKey(snapshot.root)]: [] };
    persist();
  }
  async function leaveEditor() { return !editorActive || await diffView?.canLeave() === true; }
  async function selectEntry(entry: ChangeEntry) {
    if (selected?.id === entry.id || !await leaveEditor()) return;
    selected = entry;
  }
  async function setMode(next: DiffMode) {
    if (next === mode || !await leaveEditor()) return;
    mode = next;
  }

  async function requestDiff() {
    clearTimeout(diffTimer);
    if (readingContents) { deferredDiff = true; return; }
    if (editorActive) return;
    if (!selected || !snapshot) return;
    const entryId = selected.id;
    const comparison = mode;
    const currentSession = sessionId;
    const id = ++requestId;
    loadingDiff = true;
    diffError = null;
    if (response?.result.entryId !== entryId || response.result.mode !== comparison) response = null;
    try {
      const result = await getDiff(entryId, comparison, id);
      if (id !== requestId || currentSession !== sessionId || selected?.id !== entryId || mode !== comparison) return;
      if (result.generation < (snapshot?.generation ?? 0)) return;
      response = result;
    } catch (error) {
      if (id === requestId && currentSession === sessionId) diffError = String(error);
    } finally {
      if (id === requestId && currentSession === sessionId) loadingDiff = false;
    }
  }
  async function loadContents(): Promise<DiffContents> {
    if (!response) throw new Error('Seleccioná un archivo para revelar su contexto.');
    const current = response;
    const currentSession = sessionId;
    readingContents = true;
    clearTimeout(diffTimer);
    try { return await loadDiffContents(current.result.entryId, current.result.mode, current.requestId); }
    catch (cause) {
      if (currentSession !== sessionId || selected?.id !== current.result.entryId || mode !== current.result.mode) throw cause;
      // Focus/watcher status revisions can advance between the patch and full
      // content request. Retry once against a fresh patch; DiffView still checks
      // the complete versions against its original alignment before editing.
      // Keep the same foreground request identity and do not publish a response
      // while its contents are loading: replacing it would restart hydration.
      await getDiff(current.result.entryId, current.result.mode, current.requestId);
      if (currentSession !== sessionId || selected?.id !== current.result.entryId || mode !== current.result.mode) throw cause;
      return await loadDiffContents(current.result.entryId, current.result.mode, current.requestId);
    } finally {
      readingContents = false;
      if (deferredDiff) { deferredDiff = false; queueDiff(); }
    }
  }
  function queueDiff() {
    clearTimeout(diffTimer);
    if (readingContents) { deferredDiff = true; return; }
    if (editorActive) return;
    diffTimer = setTimeout(() => void requestDiff(), 100);
  }
  function editingChanged(active: boolean) {
    editorActive = active;
    if (active) clearTimeout(diffTimer);
  }
  function handleEvent(event: WorkspaceEvent) {
    if (event.type === 'reset') {
      if (snapshot?.root !== event.snapshot.root) expanded = new Set(preferences.expandedPaths[expansionKey(event.snapshot.root)] ?? []);
      snapshot = event.snapshot;
      workspaceError = null;
      totalCount = event.snapshot.repositories.length;
      elapsedMs = null;
      if (selected) queueDiff();
      return;
    }
    if (!snapshot || event.generation !== snapshot.generation) return;
    if (event.type === 'repository') {
      const prior = snapshot.repositories.find(state => state.repository.id === event.state.repository.id);
      const incoming = event.state.error && prior ? { ...event.state, entries: prior.entries } : event.state;
      snapshot = { ...snapshot, scannedCount: event.scannedCount, repositories: [...snapshot.repositories.filter(state => state.repository.id !== event.state.repository.id), incoming] };
      totalCount = event.totalCount;
      if (selected?.repoId === incoming.repository.id) queueDiff();
    } else if (event.type === 'done') {
      snapshot = { ...snapshot, scanning: false };
      elapsedMs = event.elapsedMs;
    } else {
      workspaceError = event.message;
      snapshot = { ...snapshot, scanning: false };
    }
  }
  $effect(() => {
    const current = snapshot;
    const error = workspaceError;
    if (current) untrack(() => {
      rootBadges = { ...rootBadges, [current.root]: workspaceBadge(current, error, rootBadges[current.root]) };
    });
  });
  async function checkInactiveRoots(force = false) {
    if (!windowActive) return;
    const roots = preferences.openedRoots.filter(root => root !== snapshot?.root);
    const request = ++badgeRequest;
    try {
      const results = await getRootBadges(roots, force);
      if (request !== badgeRequest) return;
      const next = Object.fromEntries(Object.entries(rootBadges).filter(([root]) => preferences.openedRoots.includes(root)));
      for (const result of results) {
        if (result.root === snapshot?.root || !preferences.openedRoots.includes(result.root)) continue;
        const prior = next[result.root];
        next[result.root] = { hasChanges: result.hasChanges ?? (prior?.hasChanges === true ? true : null), stale: !!result.error, error: result.error };
      }
      rootBadges = next;
    } catch (cause) {
      if (request !== badgeRequest || !windowActive) return;
      const next = { ...rootBadges };
      for (const root of roots) {
        if (root === snapshot?.root || !preferences.openedRoots.includes(root)) continue;
        next[root] = { hasChanges: next[root]?.hasChanges === true ? true : null, stale: true, error: String(cause) };
      }
      rootBadges = next;
    }
  }
  $effect(() => {
    preferences.openedRoots; activeRootPath;
    if (!opening && windowActive) untrack(() => void checkInactiveRoots());
  });
  function windowFocused() {
    const wasActive = windowActive;
    windowActive = true;
    if (!isDesktop && snapshot && !opening && !snapshot.scanning) void refresh();
    else if (wasActive && !opening) void checkInactiveRoots();
  }
  function windowBlurred() { windowActive = false; ++badgeRequest; }
  async function openRoot(root: string) {
    recentMenu = false;
    if (snapshot?.root === root && !workspaceError) return;
    if (!await leaveEditor()) return;
    preferences.openedRoots = restoreRoots(preferences.openedRoots, root);
    preferences.lastRoot = root;
    persist();
    const currentSession = ++sessionId;
    ++requestId;
    clearTimeout(diffTimer);
    selected = null;
    response = null;
    loadingDiff = false;
    diffError = null;
    workspaceError = null;
    query = '';
    elapsedMs = null;
    totalCount = 0;
    expanded = new Set(preferences.expandedPaths[expansionKey(root)] ?? []);
    snapshot = { root, generation: 0, repositories: [], scanning: true, scannedCount: 0 };
    opening = true;
    try {
      const result = await openWorkspace(root, event => { if (currentSession === sessionId) handleEvent(event); });
      if (currentSession !== sessionId) return;
      if (result.generation > snapshot.generation) snapshot = result;
      totalCount = Math.max(totalCount, result.repositories.length);
      const canonicalRoot = result.root;
      preferences.openedRoots = canonicalizeTab(preferences.openedRoots, root, canonicalRoot);
      preferences.recentRoots = [canonicalRoot, ...preferences.recentRoots.filter(path => path !== canonicalRoot && path !== root)].slice(0, 8);
      preferences.lastRoot = canonicalRoot;
      persist();
      if (!snapshot.scanning) elapsedMs = result.repositories.reduce((sum, state) => sum + state.elapsedMs, 0);
    } catch (error) {
      if (currentSession !== sessionId) return;
      workspaceError = String(error);
      snapshot = { ...snapshot, scanning: false };
      // An invalid tab must not leave the previous root's watchers running.
      await closeWorkspace().catch(error => showToast(String(error)));
      persist();
    } finally {
      if (currentSession === sessionId) opening = false;
    }
  }
  async function closeRoot(root: string) {
    if (root === snapshot?.root && !await leaveEditor()) return;
    const next = closeTab(preferences.openedRoots, snapshot?.root ?? null, root);
    preferences.openedRoots = next.roots;
    persist();
    if (root !== snapshot?.root) return;
    if (next.active) { await openRoot(next.active); return; }
    ++sessionId;
    ++requestId;
    clearTimeout(diffTimer);
    snapshot = null;
    selected = null;
    response = null;
    loadingDiff = false;
    opening = false;
    diffError = null;
    workspaceError = null;
    recentMenu = false;
    preferences.lastRoot = null;
    try { await closeWorkspace(); }
    catch (error) { showToast(String(error)); }
    persist();
  }
  async function browseRoot() {
    try { const root = await chooseRoot(); if (root) await openRoot(root); }
    catch (error) { showToast(String(error)); }
  }
  async function browseRepository() {
    try {
      const path = await chooseRepository();
      if (!path) return;
      clearTimeout(saveTimer);
      await savePreferences($state.snapshot(preferences));
      await addRepository(path);
      const latest = await getPreferences();
      preferences.extraRepositories = latest.extraRepositories;
      showToast('Repositorio agregado');
    } catch (error) { showToast(String(error)); }
  }
  async function refresh() {
    if (!snapshot || snapshot.scanning || opening) return;
    void checkInactiveRoots(true);
    workspaceError = null;
    try { await refreshWorkspace(); }
    catch (error) { workspaceError = String(error); }
  }
  async function copySelection() {
    try {
      if (!await diffView?.copySelection()) showToast('Seleccioná líneas en uno de los lados para copiarlas.');
      else showToast('Selección copiada');
    } catch (error) { showToast(`No se pudo copiar la selección: ${String(error)}`); }
  }
  function keydown(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    const command = event.metaKey || event.ctrlKey;
    if (command && event.key.toLowerCase() === 'o') { event.preventDefault(); void browseRoot(); }
    else if (command && event.key.toLowerCase() === 'p') { event.preventDefault(); searchInput?.focus(); searchInput?.select(); }
    else if (command && event.key.toLowerCase() === 'r') { event.preventDefault(); void refresh(); }
    else if (command && event.shiftKey && event.key.toLowerCase() === 'w' && snapshot) { event.preventDefault(); void closeRoot(snapshot.root); }
    else if (event.ctrlKey && ['PageUp', 'PageDown'].includes(event.key) && preferences.openedRoots.length) {
      event.preventDefault();
      const roots = preferences.openedRoots;
      const index = roots.indexOf(snapshot?.root ?? '');
      void openRoot(roots[(index + (event.key === 'PageDown' ? 1 : -1) + roots.length) % roots.length]);
    }
    else if (event.key === 'F7' && !editorActive) { event.preventDefault(); diffView?.nextHunk(event.shiftKey ? -1 : 1); }
    else if (event.altKey && ['ArrowDown', 'ArrowUp'].includes(event.key)) {
      event.preventDefault();
      const index = visibleEntries.findIndex(entry => entry.id === selected?.id);
      const next = index + (event.key === 'ArrowDown' ? 1 : -1);
      if (visibleEntries[next]) selectEntry(visibleEntries[next]);
    } else if (event.key === 'Escape') {
      recentMenu = false;
      if (document.activeElement === searchInput) { query = ''; changeTree?.focusSelected(); }
    }
  }
  function startResize(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    resizing = true;
    const startX = event.clientX;
    const width = preferences.sidebarWidth;
    function move(moveEvent: PointerEvent) { preferences.sidebarWidth = Math.min(520, Math.max(220, width + moveEvent.clientX - startX)); }
    function end() {
      resizing = false;
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      persist();
    }
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
  }
  function resizeKey(event: KeyboardEvent) {
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault();
      preferences.sidebarWidth = Math.min(520, Math.max(220, preferences.sidebarWidth + (event.key === 'ArrowRight' ? 16 : -16)));
      persist();
    }
  }

  // New workspaces start fully expanded; remembered workspaces retain their tree.
  $effect(() => {
    if (snapshot && !(expansionKey(snapshot.root) in preferences.expandedPaths)) expanded = new Set(preferences.treeView === 'repositories' ? tree.map(node => node.key) : directoryPaths(tree));
  });
  $effect(() => {
    const id = selected?.id;
    const current = visibleEntries.find(entry => entry.id === id);
    if (current && current !== selected) selected = current;
    else if (!id && visibleEntries.length) {
      // The desktop starts at the first changed path; preview opens the reference.
      selected = !isDesktop ? visibleEntries.find(entry => entry.path.endsWith('deployOppen.py')) ?? visibleEntries[0] : visibleEntries[0];
    } else if (id && !editorActive && !snapshot?.scanning && !states.some(state => state.entries.some(entry => entry.id === id))) selected = null;
  });
  $effect(() => {
    const id = selected?.id; mode;
    untrack(() => {
      if (id) void requestDiff();
      else { ++requestId; clearTimeout(diffTimer); response = null; loadingDiff = false; diffError = null; }
    });
  });

  onMount(() => {
    let mounted = true;
    let removeExitListener: (() => void) | undefined;
    let exiting = false;
    void onExitRequested(() => {
      if (exiting) return;
      exiting = true;
      void (async () => {
        try {
          const canClose = diffView ? await diffView.canLeave() : !editorActive;
          if (canClose) await exitApplication();
        }
        catch (error) { showToast(String(error)); }
        finally { exiting = false; }
      })();
    }).then(remove => { if (mounted) removeExitListener = remove; else remove(); });
    void getPreferences().then(saved => {
      if (!mounted) return;
      preferences = { ...structuredClone(defaultPreferences), ...saved, sidebarWidth: Math.max(220, Math.min(520, saved.sidebarWidth || 280)) };
      preferences.openedRoots = restoreRoots(preferences.openedRoots, saved.lastRoot);
      const root = saved.lastRoot ?? preferences.openedRoots[0] ?? (!isDesktop ? '/preview/oojs' : null);
      if (root) void openRoot(root);
    }).catch(error => showToast(`No se pudieron cargar las preferencias: ${String(error)}`));
    return () => {
      mounted = false;
      removeExitListener?.();
      ++sessionId;
      ++badgeRequest;
      clearTimeout(saveTimer);
      clearTimeout(diffTimer);
      clearTimeout(toastTimer);
      void savePreferences($state.snapshot(preferences));
    };
  });
</script>

<svelte:window onkeydown={keydown} onfocus={windowFocused} onblur={windowBlurred} />
<div class="app-shell" class:desktop={isDesktop} class:resizing style:--sidebar-width={`${preferences.sidebarWidth}px`}>
  <header class="app-toolbar" data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <span class="brand-mark"><Icon name="split" size={19} /></span><span class="brand-name" data-tauri-drag-region>GitVista</span>
      {#if !isDesktop}<span class="preview-label">PREVIEW</span>{/if}
    </div>
    <div class="workspace-control" data-tauri-drag-region>
      {#if preferences.openedRoots.length}
        <RootTabs roots={preferences.openedRoots} activeRoot={snapshot?.root ?? null} badges={rootBadges} onSelect={root => void openRoot(root)} onClose={root => void closeRoot(root)} />
      {:else}<span class="toolbar-caption" data-tauri-drag-region>Un espacio para revisar tus cambios</span>{/if}
    </div>
    <div class="toolbar-actions">
      <button class="icon-button" onclick={browseRoot} aria-label="Agregar carpeta raíz" title="Agregar raíz · ⌘O"><Icon name="plus" size={18} /></button>
      <div class="root-picker">
        <button class="icon-button" onclick={() => recentMenu = !recentMenu} aria-label="Carpetas recientes" title="Carpetas recientes" aria-haspopup="menu" aria-expanded={recentMenu}><Icon name="clock" size={17} /></button>
        {#if recentMenu}<div class="recent-menu" role="menu" aria-label="Proyectos recientes">
          <div class="menu-caption">PROYECTOS RECIENTES</div>
          {#each preferences.recentRoots as root}<button role="menuitem" onclick={() => openRoot(root)} title={root}><Icon name="folder" size={15} /><span>{root.split('/').filter(Boolean).pop()}<small>{root}</small></span>{#if root === snapshot?.root}<Icon name="check" size={15} />{/if}</button>{/each}
          <button role="menuitem" class="menu-open" onclick={browseRoot}><Icon name="plus" size={15} /><span>Abrir otra carpeta</span><kbd>⌘O</kbd></button>
        </div>{/if}
      </div>
      <button class="icon-button" onclick={refresh} disabled={!snapshot || snapshot.scanning || opening} aria-label="Actualizar cambios" title="Actualizar · ⌘R"><span class:spinning={snapshot?.scanning}><Icon name="refresh" size={17} /></span></button>
    </div>
  </header>

  {#if snapshot}
    <div class="workspace-layout" id="workspace-panel">
      <aside class="sidebar">
        <div class="sidebar-heading"><span>CAMBIOS</span><span class="count-pill">{modeCount}</span><div class="heading-actions"><button class="icon-button small" onclick={collapseAll} aria-label="Contraer carpetas" title="Contraer carpetas"><Icon name="collapse" size={14} /></button><button class="icon-button small" onclick={browseRepository} aria-label="Agregar repositorio anidado" title="Agregar repositorio anidado"><Icon name="plus" size={16} /></button></div></div>
        <div class="search-field"><Icon name="search" size={15} /><input bind:this={searchInput} bind:value={query} aria-label="Buscar archivos o repositorios" placeholder="Buscar archivo o repositorio" spellcheck="false" />{#if query}<button class="search-clear" onclick={() => { query = ''; searchInput?.focus(); }} aria-label="Limpiar búsqueda"><Icon name="close" size={13} /></button>{:else}<kbd>⌘P</kbd>{/if}</div>
        <div class="sidebar-view-controls">
          <div class="tree-view-switch" aria-label="Organización del árbol">
            <button class:active={preferences.treeView === 'folders'} aria-pressed={preferences.treeView === 'folders'} onclick={() => setTreeView('folders')} title="Respetar las carpetas del proyecto"><Icon name="folder" size={13} />Carpetas</button>
            <button class:active={preferences.treeView === 'repositories'} aria-pressed={preferences.treeView === 'repositories'} onclick={() => setTreeView('repositories')} title="Mostrar cada repositorio al mismo nivel"><Icon name="repo" size={13} />Repositorios</button>
          </div>
        </div>
        {#if preferences.treeView === 'folders'}<div class="project-row" title={snapshot.root}><Icon name={rootRepo ? 'repo' : 'folder-open'} size={16} /><span>{rootName}</span>{#if rootRepo?.branch}<span class="project-branch"><Icon name="branch" size={12} />{rootRepo.branch}</span>{/if}{#if rootRepo}<RepoUntrackedToggle repository={rootRepo} show={preferences.untrackedByRepo[rootRepo.id] !== false} onToggle={setUntrackedVisibility} />{/if}</div>{/if}
        {#if tree.length}<ChangeTree bind:this={changeTree} nodes={tree} {expanded} selectedId={selected?.id ?? null} {mode} {query} visibility={preferences.untrackedByRepo} onVisibilityChange={setUntrackedVisibility} onToggle={togglePath} onSelect={selectEntry} />
        {:else}<div class="sidebar-empty"><span class="empty-tree-icon"><Icon name={snapshot.scanning ? 'refresh' : staleCount > 0 || workspaceError ? 'warning' : query ? 'search' : hiddenCount ? 'file' : 'check'} size={24} /></span><strong>{snapshot.scanning ? 'Buscando cambios…' : staleCount > 0 || workspaceError ? 'Cambios sin verificar' : query ? 'Sin coincidencias' : hiddenCount ? 'Sin archivos visibles' : 'Todo al día'}</strong><p>{snapshot.scanning ? 'Los repositorios aparecen a medida que se consultan.' : staleCount > 0 || workspaceError ? 'No pudimos consultar todos los repositorios. Actualizá para verificar los cambios.' : query ? 'Probá con parte de la ruta o del nombre del repositorio.' : hiddenCount ? `${hiddenCount} archivos sin seguimiento están ocultos. Usá el ojo del repositorio para mostrarlos.` : mode === 'staged' ? 'No hay cambios preparados.' : 'No hay archivos modificados en esta vista.'}</p>{#if query}<button class="text-button" onclick={() => query = ''}>Limpiar búsqueda</button>{/if}</div>{/if}
        <div class="sidebar-bottom"><button onclick={browseRepository} class="add-repo-button"><Icon name="plus" size={14} /><span>Agregar repositorio</span></button><span title="Repositorios descubiertos">{states.length} repos</span></div>
      </aside>
      <!-- A focusable separator implements the ARIA window splitter keyboard pattern. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div class="sidebar-resizer" role="separator" aria-label="Ancho del panel de archivos" aria-orientation="vertical" aria-valuenow={preferences.sidebarWidth} aria-valuemin={220} aria-valuemax={520} tabindex="0" onpointerdown={startResize} onkeydown={resizeKey}></div>
      <main class="diff-panel" aria-label="Vista de diferencias">
        <div class="diff-toolbar">
          <div class="file-breadcrumbs" title={selected?.absolutePath ?? snapshot.root}>
            {#if selected}<span class="breadcrumb-file-icon"><Icon name="file" size={15} /></span>{#each pathParts as part, index}<span class:breadcrumb-current={index === pathParts.length - 1}>{part}</span>{#if index < pathParts.length - 1}<Icon name="chevron-right" size={11} />{/if}{/each}
            {:else}<span class="no-file-label">Seleccioná un archivo</span>{/if}
          </div>
          <div class="diff-controls"><div class="comparison-switch" aria-label="Comparación">{#each modes as option}<button class:active={mode === option.value} aria-pressed={mode === option.value} title={option.title} onclick={() => setMode(option.value)}>{option.label}</button>{/each}</div><div class="diff-navigation"><button class="icon-button small" disabled={!response || response.result.kind !== 'text' || loadingDiff || editorActive} aria-label="Cambio anterior" title="Cambio anterior · ⇧F7" onclick={() => diffView?.nextHunk(-1)}><Icon name="chevron-up" size={16} /></button><button class="icon-button small" disabled={!response || response.result.kind !== 'text' || loadingDiff || editorActive} aria-label="Cambio siguiente" title="Cambio siguiente · F7" onclick={() => diffView?.nextHunk(1)}><Icon name="chevron-down" size={16} /></button><button class="icon-button small" disabled={!response || response.result.kind !== 'text' || loadingDiff || editorActive} aria-label="Copiar líneas seleccionadas" title="Copiar líneas seleccionadas" onclick={copySelection}><Icon name="copy" size={15} /></button></div></div>
        </div>
        <DiffView bind:this={diffView} {response} loading={loadingDiff} error={diffError} onLoadContents={loadContents} onRefresh={() => requestDiff()} onEditingChange={editingChanged} />
      </main>
    </div>
    <footer class="statusbar"><div class="status-left"><span class="status-indicator" class:busy={snapshot.scanning} class:stale={staleCount > 0 || !!workspaceError}></span>{#if workspaceError}<span class="status-error" title={workspaceError}>{workspaceError}</span>{:else if snapshot.scanning}<span>Consultando {snapshot.scannedCount}{totalCount ? ` de ${totalCount}` : ''} repositorios</span>{:else if staleCount}<span title={states.filter(state => state.error).map(state => `${state.repository.relativePath}: ${state.error}`).join('\n')}>{staleCount} repositorios sin actualizar</span>{:else}<span>{states.length} repositorios<span class="status-separator">·</span>{modeCount} archivos con cambios{#if hiddenCount}<span class="status-separator">·</span>{hiddenCount} sin seguimiento ocultos{/if}</span>{/if}{#if elapsedMs !== null && !snapshot.scanning}<span class="scan-time">{elapsedMs < 1000 ? `${Math.round(elapsedMs)} ms` : `${(elapsedMs / 1000).toFixed(1)} s`}</span>{/if}</div><div class="status-right">{#if currentRepo?.branch}<span><Icon name="branch" size={12} />{currentRepo.branch}</span><span class="status-separator">·</span>{/if}<span><Icon name="split" size={12} />Vista dividida</span><span class="status-separator">·</span><span>{editorActive ? "Editando archivo actual" : "Revisión"}</span></div></footer>
  {:else}
    <main class="welcome-screen">
      <div class="welcome-art" aria-hidden="true"><div class="welcome-pane old"><span></span><span></span><span class="removed"></span><span></span><span></span></div><div class="welcome-art-divider"></div><div class="welcome-pane new"><span></span><span></span><span class="added"></span><span></span><span></span></div><span class="welcome-app-icon"><Icon name="split" size={27} /></span></div>
      <div class="welcome-eyebrow">GITVISTA</div><h1>Tus cambios,<br /><span>sin distracciones.</span></h1><p>Todos tus repositorios, un solo árbol.<br />Una vista clara de lo que cambió.</p><button class="primary-button" onclick={browseRoot}><Icon name="folder-open" size={17} />Abrir carpeta de proyecto<kbd>⌘O</kbd></button>
      {#if preferences.recentRoots.length}<div class="welcome-recents"><span>RECIENTES</span>{#each preferences.recentRoots.slice(0, 3) as root}<button onclick={() => openRoot(root)} title={root}><Icon name="clock" size={14} /><span>{root.split('/').filter(Boolean).pop()}</span><small>{root}</small><Icon name="chevron-right" size={14} /></button>{/each}</div>{/if}
      <span class="welcome-footnote">Ligero por diseño. Revisá cambios y editá cuando lo necesites.</span>
    </main>
  {/if}
  {#if toast}<div class="toast" role="status"><Icon name="check" size={16} />{toast}</div>{/if}
  {#if recentMenu}<button class="menu-backdrop" aria-label="Cerrar proyectos recientes" tabindex="-1" onclick={() => recentMenu = false}></button>{/if}
</div>
