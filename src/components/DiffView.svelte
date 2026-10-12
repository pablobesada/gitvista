<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { CodeView } from '@pierre/diffs';
  import type { CodeViewOptions, FileDiffLoadedFiles, FileDiffMetadata, SelectedLineRange } from '@pierre/diffs';
  import { WorkerPoolManager } from '@pierre/diffs/worker';
  import DiffsWorker from '@pierre/diffs/worker/worker.js?worker';
  import { copyText, readClipboardText, saveWorkingFile, setEditorActive } from '../lib/bridge';
  import Icon from './Icon.svelte';
  import {
    changeTargets, completeDiff, nativeSelectionText, nativeTextRange, parseSelectedDiff,
    selectedLinesText, selectionFromContents, validateLoadedContents,
  } from '../lib/diff-view';
  import type { DiffContents, DiffResponse } from '../lib/types';

  let { response, loading = false, error = null, onLoadContents, onRefresh, onEditingChange }: {
    response: DiffResponse | null;
    loading?: boolean;
    error?: string | null;
    onLoadContents: () => Promise<DiffContents>;
    onRefresh: () => Promise<void>;
    onEditingChange: (editing: boolean) => void;
  } = $props();

  let viewport: HTMLDivElement;
  let mounted = $state(false);
  let renderError = $state<string | null>(null);
  let contentsLoading = $state(false);
  let selectionHint = $state('Seleccioná números de línea para copiar un rango · ⌘C');
  let activeHunk = $state(-1);
  let hunkCount = $state(0);
  let scope = $state<'changes' | 'full'>('changes');
  let fullReady = $state(false);
  let editing = $state(false);
  let dirty = $state(false);
  let saving = $state(false);
  let editStarting = $state(false);
  let discardDialog = $state(false);
  let keepEditingButton = $state<HTMLButtonElement>();
  let leavePromise: Promise<boolean> | null = null;
  let resolveLeave: ((leave: boolean) => void) | null = null;
  let draftText = '';
  let baselineText = '';
  let itemVersion = 0;
  let editorClass: typeof import('@pierre/diffs/edit').Editor | undefined;
  let rendererOptions: CodeViewOptions<undefined, undefined>;
  let viewer: CodeView | undefined;
  let workerPool: WorkerPoolManager | undefined;
  let activeModel: FileDiffMetadata | null = null;
  let activeResponse: DiffResponse | null = null;
  let activeItemId = '';
  let activeKey = '';
  let revision = 0;
  let contents: DiffContents | null = null;
  let contentsPromise: Promise<DiffContents> | null = null;
  let contentLoader: () => Promise<DiffContents> = () => onLoadContents();

  const rendererCSS = `
    :host {
      --diffs-font-family: "SF Mono", "Menlo", "Consolas", monospace;
      --diffs-font-size: 12px;
      --diffs-line-height: 20px;
      --diffs-bg: #1e1e1e;
      --diffs-fg: #d4d4d4;
      --diffs-fg-number: #717171;
      --diffs-deletion-base: #d98486;
      --diffs-addition-base: #92b88b;
      --diffs-bg-deletion: #4b2528;
      --diffs-bg-addition: #2c3d28;
      --diffs-bg-deletion-emphasis: #783239;
      --diffs-bg-addition-emphasis: #436539;
      --diffs-bg-buffer: #191b1d;
      --diffs-bg-hover: #ffffff05;
      --diffs-bg-selection: #3a4964;
      --diffs-fg-selection: #e2e7ef;
    }
    [data-diff-type="split"] { gap: 1px; background: #ffffff0b; }
    /* Pierre omits the missing side for additions/deletions; keep its column. */
    [data-diff-type="single"][data-overflow="scroll"] {
      display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }
    [data-diff-type="single"] > [data-additions] { grid-column: 2; border-left: 1px solid #ffffff0b; }
    [data-diff-type="single"] > [data-deletions] { grid-column: 1; border-right: 1px solid #ffffff0b; }
    [data-code] { background: #1e1e1e; tab-size: 4; }
    [data-number] { color: #717171; }
    [data-line]::selection, [data-line] ::selection { background: #384d6e; }
    [data-separator] { color: #848da0; }
  `;

  function renderer(): CodeView {
    if (viewer != null) return viewer;
    workerPool = new WorkerPoolManager({
      workerFactory: () => new DiffsWorker(),
      poolSize: 1,
      totalASTLRUCacheSize: 2,
      onWorkerError: (event) => {
        event.preventDefault();
        selectionHint = 'Resaltado en modo local · Seleccioná líneas para copiar';
      },
    }, {
      theme: 'dark-plus',
      langs: [],
      preferredHighlighter: 'shiki-js',
      tokenizeMaxLineLength: 2_000,
      maxLineDiffLength: 2_000,
      lineDiffType: 'word-alt',
    });
    const options: CodeViewOptions<undefined, undefined> = {
      theme: 'dark-plus', themeType: 'dark', diffStyle: 'split', overflow: 'scroll',
      diffIndicators: 'none', disableFileHeader: true, disableLineNumbers: false,
      expandUnchanged: false, expansionLineCount: 20, collapsedContextThreshold: 3,
      hunkSeparators: 'line-info', lineDiffType: 'word-alt',
      maxLineDiffLength: 2_000, tokenizeMaxLineLength: 2_000, tokenizeMaxLength: 20_000,
      preferredHighlighter: 'shiki-js', unsafeCSS: rendererCSS,
      enableLineSelection: true, stickyHeaders: false,
      layout: { paddingTop: 0, paddingBottom: 0, gap: 0 },
      itemMetrics: { lineHeight: 20, spacing: 8, paddingTop: 8, paddingBottom: 16 },
      loadDiffFiles: async (model) => {
        const loaded = await loadContents(model);
        if (model.type === 'rename-pure' && loaded.newFile != null) {
          return { oldFile: null, newFile: loaded.newFile };
        }
        if (loaded.oldFile == null || loaded.newFile == null) {
          throw new Error('Las versiones del archivo cambiaron. Actualizá el diff para ampliar el contexto.');
        }
        return loaded as FileDiffLoadedFiles;
      },
      onSelectedLinesChange: (selection) => {
        if (selection == null) {
          selectionHint = 'Seleccioná números de línea para copiar un rango · ⌘C';
        } else {
          const { start, end, side } = selection.range;
          selectionHint = `${side === 'deletions' ? 'Viejo' : 'Nuevo'} · líneas ${Math.min(start, end)}–${Math.max(start, end)} · ⌘C para copiar`;
        }
      },
      createEditor: (type, options) => {
        if (!editorClass) throw new Error('El editor aún no está listo.');
        return new editorClass(type, { ...options, historyMaxEntries: 50,
          clipboard: { readText: readClipboardText },
          onAttach: editor => editor.focus({ lineNumber: 'first-visible', preventScroll: true }),
        });
      },
      onItemEditChange: event => {
        draftText = event.file.contents;
        dirty = draftText !== baselineText;
      },
      // Save goes through the native write command; cancellation restores Git's model.
      onItemEditComplete: () => 'reject',
    };
    rendererOptions = options;
    viewer = new CodeView(options, workerPool);
    viewer.setup(viewport);
    return viewer;
  }

  async function loadContents(model: FileDiffMetadata | null): Promise<DiffContents> {
    if (model !== activeModel) throw new DOMException('Diff reemplazado', 'AbortError');
    if (contents != null) return contents;
    if (contentsPromise != null) return contentsPromise;
    const expectedRevision = revision;
    const loader = contentLoader;
    contentsLoading = true;
    const pending = loader().then((loaded) => {
      if (revision !== expectedRevision || model !== activeModel) {
        throw new DOMException('Diff reemplazado', 'AbortError');
      }
      if (model) validateLoadedContents(model, loaded);
      contents = loaded;
      return loaded;
    }).catch((cause: unknown) => {
      if (revision === expectedRevision) renderError = cause instanceof Error ? cause.message : String(cause);
      throw cause;
    }).finally(() => {
      if (revision === expectedRevision) { contentsLoading = false; contentsPromise = null; }
    });
    contentsPromise = pending;
    return pending;
  }

  function install(next: DiffResponse | null, loader: () => Promise<DiffContents>): void {
    // Watcher refreshes must never replace an active draft.
    if (editing || editStarting) return;
    const key = next == null ? '' : `${next.generation}:${next.requestId}:${next.result.entryId}:${next.result.mode}`;
    if (key === activeKey) return;
    const sameFile = next != null && activeResponse != null &&
      next.result.entryId === activeResponse.result.entryId && next.result.mode === activeResponse.result.mode;
    revision++;
    activeKey = key;
    activeResponse = next;
    contentLoader = loader;
    contents = null;
    contentsPromise = null;
    contentsLoading = false;
    renderError = null;
    selectionHint = 'Seleccioná números de línea para copiar un rango · ⌘C';
    activeModel = null;
    fullReady = false;
    hunkCount = 0;
    if (!sameFile) activeHunk = -1;
    if (next == null || next.result.kind !== 'text') {
      viewer?.setItems([]);
      return;
    }
    try {
      activeModel = parseSelectedDiff(next);
      if (activeModel == null) return;
      hunkCount = activeModel.hunks.length;
      activeHunk = Math.min(activeHunk, hunkCount - 1);
      activeItemId = `${next.result.entryId}:${next.result.mode}`;
      const instance = renderer();
      instance.clearSelectedLines();
      instance.setItems([{ id: activeItemId, type: 'diff', fileDiff: activeModel, version: ++itemVersion }]);
      if (!sameFile) instance.scrollTo({ type: 'position', position: 0, behavior: 'instant' });
    } catch (cause) {
      renderError = cause instanceof Error ? cause.message : String(cause);
      viewer?.setItems([]);
    }
  }

  $effect(() => {
    const next = response;
    const loader = onLoadContents;
    if (mounted) untrack(() => install(next, loader));
  });

  async function showFull(): Promise<void> {
    if (!activeResponse || !['text', 'empty', 'metadata'].includes(activeResponse.result.kind)) return;
    if (fullReady && activeModel && contents) {
      viewer?.setOptions({ ...rendererOptions, expandUnchanged: scope === 'full' });
      return;
    }
    const expected = revision;
    const loaded = await loadContents(activeModel);
    if (expected !== revision) return;
    if (activeModel && activeModel.type !== 'rename-pure' && loaded.oldFile && loaded.newFile) {
      // CodeView owns hydration and its highlight/layout caches. Mutating its
      // mounted metadata here would leave the previous partial AST out of sync.
      fullReady = true;
      renderer().setOptions({ ...rendererOptions, expandUnchanged: scope === 'full' });
      return;
    }
    activeModel = completeDiff(activeModel, loaded);
    activeItemId = `${activeResponse.result.entryId}:${activeResponse.result.mode}`;
    hunkCount = changeTargets(activeModel).length;
    fullReady = true;
    const instance = renderer();
    instance.setOptions({ ...rendererOptions, expandUnchanged: scope === 'full' });
    instance.setItems([{ id: activeItemId, type: 'diff', fileDiff: activeModel, version: ++itemVersion }]);
  }
  async function setScope(next: 'changes' | 'full') {
    if (editing || editStarting) return;
    scope = next;
    renderError = null;
    try {
      if (next === 'changes') viewer?.setOptions({ ...rendererOptions, expandUnchanged: false });
    } catch (cause) { renderError = String(cause); }
  }
  $effect(() => {
    response; scope; mounted;
    if (mounted && scope === 'full') untrack(() => { if (!editing && !editStarting) void showFull().catch(cause => renderError = String(cause)); });
  });

  async function beginEdit() {
    if (!activeResponse || editing || editStarting || saving) return;
    const expected = revision;
    editStarting = true;
    onEditingChange(true);
    renderError = null;
    try {
      const loaded = await loadContents(activeModel);
      if (expected !== revision) return;
      if (!loaded.editVersion || !loaded.newFile) throw new Error(loaded.editUnavailableReason ?? 'La versión nueva no se puede editar en esta comparación.');
      editorClass ??= (await import('@pierre/diffs/edit')).Editor;
      if (expected !== revision) return;
      await setEditorActive(true);
      if (expected !== revision) { await setEditorActive(false); return; }
      scope = 'full';
      await showFull();
      baselineText = loaded.newFile.contents;
      draftText = baselineText;
      dirty = false;
      editing = true;
      onEditingChange(true);
      viewer!.setItems([{ id: activeItemId, type: 'diff', fileDiff: activeModel!, edit: true, version: ++itemVersion }]);
    } catch (cause) {
      renderError = cause instanceof Error ? cause.message : String(cause);
      editing = false;
      onEditingChange(false);
      await setEditorActive(false);
    }
    finally {
      editStarting = false;
      if (!editing) { onEditingChange(false); install(response, onLoadContents); }
    }
  }
  async function endEdit() {
    editing = false;
    dirty = false;
    draftText = '';
    baselineText = '';
    onEditingChange(false);
    if (activeModel) viewer?.setItems([{ id: activeItemId, type: 'diff', fileDiff: activeModel, edit: false, version: ++itemVersion }]);
    await setEditorActive(false);
    install(response, onLoadContents);
    if (scope === 'full') await showFull().catch(cause => renderError = String(cause));
  }
  async function save() {
    if (!editing || saving || !activeResponse || !contents?.editVersion) return;
    if (!dirty) { await endEdit(); return; }
    saving = true;
    renderError = null;
    try {
      const text = viewer?.getEditor(activeItemId)?.getText() ?? draftText;
      await saveWorkingFile(activeResponse.result.entryId, contents.editVersion, text);
      // Keep the draft until the write succeeds. Status refreshes are read queries.
      await endEdit();
      await onRefresh();
      selectionHint = 'Archivo guardado · ⌘S';
    } catch (cause) { renderError = cause instanceof Error ? cause.message : String(cause); }
    finally { saving = false; }
  }
  export function canLeave(): Promise<boolean> {
    if (saving || editStarting) return Promise.resolve(false);
    if (!editing) return Promise.resolve(true);
    if (!dirty) return endEdit().then(() => true);
    if (leavePromise) return leavePromise;
    discardDialog = true;
    leavePromise = new Promise(resolve => resolveLeave = resolve);
    return leavePromise;
  }
  async function answerDiscard(discard: boolean) {
    discardDialog = false;
    if (discard) await endEdit();
    resolveLeave?.(discard);
    resolveLeave = null;
    leavePromise = null;
  }
  function editKeydown(event: KeyboardEvent) {
    if (discardDialog) return;
    if (editing && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
      event.preventDefault(); event.stopPropagation(); void save();
    }
  }
  function beforeUnload(event: BeforeUnloadEvent) { if (editing && dirty) { event.preventDefault(); event.returnValue = ''; } }
  $effect(() => { if (discardDialog) void tick().then(() => keepEditingButton?.focus()); });
  function discardKeydown(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); void answerDiscard(false); }
    else if (event.key === 'Tab') {
      const dialog = event.currentTarget as HTMLElement;
      const buttons = [...dialog.querySelectorAll<HTMLButtonElement>('button')];
      const index = buttons.indexOf(dialog.ownerDocument.activeElement as HTMLButtonElement);
      event.preventDefault();
      buttons[(index + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length]?.focus();
    }
  }

  export function nextHunk(direction: 1 | -1): void {
    if (activeModel == null || viewer == null) return;
    const targets = changeTargets(activeModel);
    if (targets.length === 0) return;
    activeHunk = activeHunk < 0 ? (direction > 0 ? 0 : targets.length - 1) :
      (activeHunk + direction + targets.length) % targets.length;
    viewer.scrollTo({ type: 'line', id: activeItemId, ...targets[activeHunk], align: 'center', behavior: 'instant' });
  }

  function synchronousSelectionText(): string | null {
    if (activeModel == null || viewer == null) return null;
    const native = nativeTextRange(viewport);
    if (native != null) return nativeSelectionText(activeModel, native);
    const selection = viewer.getSelectedLines();
    if (selection == null || selection.id !== activeItemId) return null;
    if (selection.range.endSide != null && selection.range.endSide !== selection.range.side) return null;
    return selectedLinesText(activeModel, selection.range) ??
      (contents == null ? null : selectionFromContents(contents, selection.range));
  }

  export async function copySelection(): Promise<boolean> {
    if (activeModel == null || viewer == null) return false;
    const expectedRevision = revision;
    let text = synchronousSelectionText();
    if (text == null) {
      const selection = viewer.getSelectedLines();
      if (selection == null || selection.id !== activeItemId) return false;
      const range: SelectedLineRange = { ...selection.range };
      if (range.endSide != null && range.endSide !== range.side) return false;
      text = selectionFromContents(await loadContents(activeModel), range);
    }
    if (text == null || expectedRevision !== revision) return false;
    await copyText(text);
    selectionHint = 'Selección copiada';
    return true;
  }

  function handleCopy(event: ClipboardEvent): void {
    if (editing) return;
    const focused = viewport.ownerDocument.activeElement;
    if (focused == null || !viewport.contains(focused)) return;
    const text = synchronousSelectionText();
    if (text != null && event.clipboardData != null) {
      event.preventDefault();
      event.clipboardData.setData('text/plain', text);
      selectionHint = 'Selección copiada';
      return;
    }
    if (viewer?.getSelectedLines() != null) {
      event.preventDefault();
      void copySelection().catch((cause: unknown) => {
        renderError = cause instanceof Error ? cause.message : String(cause);
      });
    }
  }

  onMount(() => {
    mounted = true;
    viewport.ownerDocument.addEventListener('copy', handleCopy);
    return () => {
      mounted = false;
      revision++;
      viewport.ownerDocument.removeEventListener('copy', handleCopy);
      viewer?.cleanUp();
      workerPool?.terminate();
      viewer = undefined;
      workerPool = undefined;
      activeModel = null;
      contents = null;
      resolveLeave?.(false);
    };
  });

  const leftLabel = $derived(response?.result.mode === 'unstaged' ? 'Índice' : 'HEAD');
  const rightLabel = $derived(response?.result.mode === 'staged' ? 'Índice' : 'Actual');
  const textAvailable = $derived(response != null && (response.result.kind === 'text' || (scope === 'full' && fullReady && ['empty', 'metadata'].includes(response.result.kind))));
  const canShowFull = $derived(response != null && ['text', 'empty', 'metadata'].includes(response.result.kind));
  const stateTitle = $derived(error ? 'No se pudo cargar el diff' : response == null ? 'Elegí un archivo para revisar' :
    response.result.kind === 'binary' ? 'Archivo binario' : response.result.kind === 'conflict' ? 'Conflicto de merge' :
    response.result.kind === 'tooLarge' ? 'Archivo demasiado grande' : response.result.kind === 'metadata' ? 'Cambios de metadatos' :
    response.result.kind === 'empty' ? 'Sin cambios en esta comparación' : 'No se pudo mostrar el diff');
</script>

<svelte:window onkeydown={editKeydown} onbeforeunload={beforeUnload} />

<div class="diff-view" aria-busy={loading || contentsLoading}>
  <div class="scope-bar">
    <div class="scope-switch" aria-label="Contenido del diff">
      <button class:active={scope === 'changes'} aria-pressed={scope === 'changes'} disabled={!canShowFull || loading || editing || editStarting} onclick={() => setScope('changes')}>Cambios</button>
      <button class:active={scope === 'full'} aria-pressed={scope === 'full'} disabled={!canShowFull || loading || editing || editStarting} onclick={() => setScope('full')}>Archivo completo</button>
    </div>
    <div class="edit-actions">
      {#if editing}
        <span class="draft-status">{dirty ? 'Sin guardar' : 'Editando'}</span>
        <button disabled={saving} onclick={async () => { if (await canLeave()) { renderError = null; await onRefresh(); } }}>Cancelar</button>
        <button class="save-button" disabled={saving || !dirty} onclick={save}>{saving ? 'Guardando…' : 'Guardar'}<kbd>⌘S</kbd></button>
      {:else}
        <button disabled={!canShowFull || response?.result.mode === 'staged' || loading || contentsLoading || editStarting} title={response?.result.mode === 'staged' ? 'Usá Total o Sin preparar para editar el archivo actual' : 'Editar el archivo actual en la columna derecha'} onclick={beginEdit}><Icon name="edit" size={13} />{editStarting ? 'Cargando…' : 'Editar versión nueva'}</button>
      {/if}
    </div>
  </div>
  <div class="versions" aria-hidden="true">
    <div><span class="version-dot old"></span><span>Viejo</span><span class="revision">{leftLabel}</span></div>
    <div><span class="version-dot new"></span><span>Nuevo</span><span class="revision">{rightLabel}</span></div>
  </div>
  <div class="viewer-shell">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (The scroll region needs keyboard focus for scrolling and copying code.) -->
    <div bind:this={viewport} class="viewport" tabindex="0" role="region" aria-label="Diff dividido: versión vieja a la izquierda y nueva a la derecha" class:inactive={!textAvailable || error != null}></div>
    {#if loading || contentsLoading}<div class="progress" role="status" aria-label={contentsLoading ? 'Cargando contexto del archivo' : 'Cargando diff'}></div>{/if}
    {#if error != null || !textAvailable}
      <div class="empty-state">
        <div class="state-icon" aria-hidden="true">{error ? '!' : response == null ? '⇄' : response.result.kind === 'binary' ? '◈' : response.result.kind === 'conflict' ? '⑂' : '⇄'}</div>
        <h2>{loading && response == null ? 'Cargando cambios…' : stateTitle}</h2>
        <p>{error ?? response?.result.message ?? (response == null ? 'Abrí un proyecto y recorré sus cambios desde el árbol de la izquierda.' : 'Git no devolvió cambios de texto para esta comparación.')}</p>
        {#if response?.result.kind === 'metadata' && response.result.patch}
          <pre class="metadata">{response.result.patch}</pre>
        {/if}
      </div>
    {/if}
    {#if renderError}
      <div class="inline-error" role="alert">{renderError}</div>
    {/if}
  </div>
  {#if textAvailable && !error}
    <div class="diff-footer"><span>{contentsLoading ? 'Cargando más contexto…' : selectionHint}</span><span>{activeHunk >= 0 ? `${activeHunk + 1} / ` : ''}{hunkCount} {hunkCount === 1 ? 'bloque' : 'bloques'}</span></div>
  {/if}
</div>

{#if discardDialog}
  <div class="discard-backdrop"><div class="discard-dialog" role="alertdialog" aria-modal="true" aria-labelledby="discard-title" aria-describedby="discard-description" tabindex="-1" onkeydown={discardKeydown}>
    <h2 id="discard-title">Tenés cambios sin guardar</h2><p id="discard-description">Descartá el borrador para continuar, o seguí editando y guardalo con ⌘S.</p>
    <div><button bind:this={keepEditingButton} onclick={() => answerDiscard(false)}>Seguir editando</button><button class="discard-button" onclick={() => answerDiscard(true)}>Descartar cambios</button></div>
  </div></div>
{/if}

<style>
  .diff-view { display: flex; flex: 1; flex-direction: column; min-height: 0; min-width: 0; height: 100%; background: #1e1e1e; }
  .scope-bar { min-height: 40px; display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; padding: 5px 12px; border-bottom: 1px solid #ffffff08; background: #1a1c20; font-size: 10px; }
  .scope-switch { display: flex; gap: 3px; background: #121417; padding: 3px; border-radius: 5px; }
  .scope-switch button, .edit-actions button { padding: 5px 8px; border-radius: 4px; color: #8995aa; display: flex; align-items: center; gap: 6px; font-size: 10px; }
  .scope-switch button.active { color: #d1d9ef; background: #2a2e38; }
  .edit-actions { display: flex; align-items: center; gap: 6px; }
  .edit-actions button:hover:not(:disabled) { background: #ffffff08; }
  .edit-actions .save-button { color: #ced6ff; background: #a7b0f919; }
  .draft-status { color: #c3ab78; font-size: 9px; }
  .discard-backdrop { position: fixed; inset: 0; background: #0008; display: grid; place-items: center; z-index: 100; }
  .discard-dialog { width: min(420px, 90vw); padding: 24px; border: 1px solid #ffffff16; border-radius: 12px; background: #23262d; box-shadow: 0 20px 60px #0006; }
  .discard-dialog p { color: #98a3b5; font-size: 12px; line-height: 1.7; }
  .discard-dialog > div { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }
  .discard-dialog button { padding: 8px 10px; border-radius: 5px; font-size: 12px; background: #ffffff08; }
  .discard-dialog .discard-button { color: #ffc0c3; background: #c478791c; }
  .versions { display: grid; grid-template-columns: 1fr 1fr; flex: 0 0 35px; border-bottom: 1px solid #ffffff09; background: #1c1e21; color: #acb3c0; font-size: 11px; }
  .versions > div { display: flex; align-items: center; gap: 7px; padding: 0 16px; }
  .versions > div + div { border-left: 1px solid #ffffff09; }
  .version-dot { width: 5px; height: 5px; border-radius: 50%; }
  .version-dot.old { background: #c47879; } .version-dot.new { background: #8aa980; }
  .revision { margin-left: auto; font: 10px "SF Mono", Menlo, monospace; color: #747e8e; }
  .viewer-shell { position: relative; flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .viewport { height: 100%; width: 100%; overflow: auto; overscroll-behavior: contain; scrollbar-gutter: stable; scrollbar-color: #55585b transparent; background: #1e1e1e; outline: none; }
  .viewport.inactive { visibility: hidden; }
  .viewport:focus-visible { box-shadow: inset 0 0 0 1px #a7b0f94d; }
  .progress { position: absolute; top: 0; left: 0; right: 0; height: 2px; background: linear-gradient(90deg, transparent 0%, #a7b0f9 50%, transparent 100%); background-size: 200% 100%; animation: progress 1.2s linear infinite; z-index: 5; }
  .empty-state { position: absolute; inset: 0; display: flex; flex-direction: column; justify-content: center; align-items: center; padding: 32px; text-align: center; }
  .state-icon { width: 56px; height: 56px; display: grid; place-items: center; margin-bottom: 18px; border: 1px solid #ffffff0a; border-radius: 15px; background: #ffffff03; color: #949dad; font-size: 27px; }
  h2 { margin: 0; color: #dce1e9; font-size: 15px; font-weight: 500; }
  .empty-state p { max-width: 420px; margin: 11px 0 0; color: #87909e; font-size: 12px; line-height: 1.8; overflow-wrap: anywhere; }
  .metadata { max-width: min(800px, 100%); max-height: 240px; margin-top: 24px; padding: 16px; text-align: left; overflow: auto; color: #adb6c5; border: 1px solid #ffffff08; border-radius: 8px; font: 11px/1.8 "SF Mono", Menlo, monospace; }
  .inline-error { position: absolute; top: 8px; right: 20px; left: 20px; padding: 9px 12px; border: 1px solid #ba747944; border-radius: 6px; background: #39282b; color: #e0afb1; font-size: 12px; z-index: 10; }
  .diff-footer { display: flex; justify-content: space-between; gap: 16px; flex: 0 0 26px; align-items: center; padding: 0 14px; border-top: 1px solid #ffffff08; background: #1b1d20; color: #758091; font-size: 10px; }
  .diff-footer span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  @keyframes progress { from { background-position: 200% 0; } to { background-position: 0 0; } }
  @media (prefers-reduced-motion: reduce) { .progress { animation: none; } }
</style>
