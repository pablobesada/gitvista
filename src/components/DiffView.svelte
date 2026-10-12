<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { CodeView } from '@pierre/diffs';
  import type { CodeViewOptions, FileDiffLoadedFiles, FileDiffMetadata, SelectedLineRange } from '@pierre/diffs';
  import { WorkerPoolManager } from '@pierre/diffs/worker';
  import DiffsWorker from '@pierre/diffs/worker/worker.js?worker';
  import { copyText } from '../lib/bridge';
  import {
    changeTargets, nativeSelectionText, nativeTextRange, parseSelectedDiff,
    selectedLinesText, selectionFromContents, validateLoadedContents,
  } from '../lib/diff-view';
  import type { DiffContents, DiffResponse } from '../lib/types';

  let { response, loading = false, error = null, onLoadContents }: {
    response: DiffResponse | null;
    loading?: boolean;
    error?: string | null;
    onLoadContents: () => Promise<DiffContents>;
  } = $props();

  let viewport: HTMLDivElement;
  let mounted = $state(false);
  let renderError = $state<string | null>(null);
  let contentsLoading = $state(false);
  let selectionHint = $state('Seleccioná números de línea para copiar un rango · ⌘C');
  let activeHunk = $state(-1);
  let hunkCount = $state(0);
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
    };
    viewer = new CodeView(options, workerPool);
    viewer.setup(viewport);
    return viewer;
  }

  async function loadContents(model: FileDiffMetadata): Promise<DiffContents> {
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
      validateLoadedContents(model, loaded);
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
      instance.setItems([{ id: activeItemId, type: 'diff', fileDiff: activeModel, version: next.requestId }]);
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
    };
  });

  const leftLabel = $derived(response?.result.mode === 'unstaged' ? 'Índice' : 'HEAD');
  const rightLabel = $derived(response?.result.mode === 'staged' ? 'Índice' : 'Actual');
  const stateTitle = $derived(error ? 'No se pudo cargar el diff' : response == null ? 'Elegí un archivo para revisar' :
    response.result.kind === 'binary' ? 'Archivo binario' : response.result.kind === 'conflict' ? 'Conflicto de merge' :
    response.result.kind === 'tooLarge' ? 'Archivo demasiado grande' : response.result.kind === 'metadata' ? 'Cambios de metadatos' :
    response.result.kind === 'empty' ? 'Sin cambios en esta comparación' : 'No se pudo mostrar el diff');
</script>

<div class="diff-view" aria-busy={loading || contentsLoading}>
  <div class="versions" aria-hidden="true">
    <div><span class="version-dot old"></span><span>Viejo</span><span class="revision">{leftLabel}</span></div>
    <div><span class="version-dot new"></span><span>Nuevo</span><span class="revision">{rightLabel}</span></div>
  </div>
  <div class="viewer-shell">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (The scroll region needs keyboard focus for scrolling and copying code.) -->
    <div bind:this={viewport} class="viewport" tabindex="0" role="region" aria-label="Diff dividido: versión vieja a la izquierda y nueva a la derecha" class:inactive={response == null || response.result.kind !== 'text' || error != null}></div>
    {#if loading || contentsLoading}<div class="progress" role="status" aria-label={contentsLoading ? 'Cargando contexto del archivo' : 'Cargando diff'}></div>{/if}
    {#if error != null || response == null || response.result.kind !== 'text'}
      <div class="empty-state">
        <div class="state-icon" aria-hidden="true">{error ? '!' : response == null ? '⇄' : response.result.kind === 'binary' ? '◈' : response.result.kind === 'conflict' ? '⑂' : '⇄'}</div>
        <h2>{loading && response == null ? 'Cargando cambios…' : stateTitle}</h2>
        <p>{error ?? response?.result.message ?? (response == null ? 'Abrí un proyecto y recorré sus cambios desde el árbol de la izquierda.' : 'Git no devolvió cambios de texto para esta comparación.')}</p>
        {#if response?.result.kind === 'metadata' && response.result.patch}
          <pre class="metadata">{response.result.patch}</pre>
        {/if}
      </div>
    {:else if renderError}
      <div class="inline-error" role="alert">{renderError}</div>
    {/if}
  </div>
  {#if response?.result.kind === 'text' && !error}
    <div class="diff-footer"><span>{contentsLoading ? 'Cargando más contexto…' : selectionHint}</span><span>{activeHunk >= 0 ? `${activeHunk + 1} / ` : ''}{hunkCount} {hunkCount === 1 ? 'bloque' : 'bloques'}</span></div>
  {/if}
</div>

<style>
  .diff-view { display: flex; flex: 1; flex-direction: column; min-height: 0; min-width: 0; height: 100%; background: #1e1e1e; }
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
