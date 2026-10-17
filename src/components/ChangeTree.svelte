<script lang="ts">
  import type { ChangeEntry, DiffMode, Repository } from '../lib/types';
  import { flattenTree, statusLabel, type ChangeNode, type TreeRow } from '../lib/tree';
  import Icon from './Icon.svelte';
  import RepoUntrackedToggle from './RepoUntrackedToggle.svelte';
  let { nodes, expanded, selectedId, mode, query = '', visibility, onVisibilityChange, onToggle, onSelect, onRepositoryMenu }: {
    nodes: ChangeNode[]; expanded: ReadonlySet<string>; selectedId: string | null; mode: DiffMode; query?: string;
    visibility: Record<string, boolean>; onVisibilityChange: (repoId: string, show: boolean) => void;
    onRepositoryMenu: (repository: Repository, x: number, y: number) => void;
    onToggle: (path: string) => void; onSelect: (entry: ChangeEntry) => void;
  } = $props();
  const rowHeight = 30;
  const overscan = 12;
  let scroller: HTMLDivElement;
  let scrollTop = $state(0);
  let height = $state(500);
  let focusKey = $state<string | null>(null);
  let lastRevealedId: string | null = null;
  const rows = $derived(flattenTree(nodes, expanded, query.trim().length > 0));
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + height) / rowHeight) + overscan));
  const visibleRows = $derived(rows.slice(start, end));
  $effect(() => { query; if (scroller) { scroller.scrollTop = 0; scrollTop = 0; } });
  $effect(() => {
    const id = selectedId;
    const index = rows.findIndex(row => row.node.entry?.id === id);
    if (!id) lastRevealedId = null;
    else if (id !== lastRevealedId && index >= 0) {
      reveal(index);
      lastRevealedId = id;
    }
  });

  function reveal(index: number) {
    if (!scroller) return;
    const top = index * rowHeight + parseFloat(getComputedStyle(scroller).paddingTop || '0');
    const viewportHeight = scroller.clientHeight;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + rowHeight > scroller.scrollTop + viewportHeight) scroller.scrollTop = top + rowHeight - viewportHeight;
    scrollTop = scroller.scrollTop;
  }
  function activate(row: TreeRow) {
    focusKey = row.node.key;
    scroller?.focus({ preventScroll: true });
    if (row.node.entry) onSelect(row.node.entry);
    else onToggle(row.node.key);
  }
  function focusRow(index: number, select = true) {
    const row = rows[index];
    if (!row) return;
    focusKey = row.node.key;
    reveal(index);
    if (select && row.node.entry) onSelect(row.node.entry);
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      const row = rows.find(row => row.node.key === focusKey);
      if (row?.node.repository) {
        event.preventDefault();
        const bounds = document.getElementById('node-' + encodeURIComponent(row.node.key))?.getBoundingClientRect();
        onRepositoryMenu(row.node.repository, bounds?.left ?? 20, bounds?.bottom ?? 120);
      }
      return;
    }
    if (!['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Enter', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    let index = rows.findIndex(row => row.node.key === focusKey);
    if (index < 0) index = rows.findIndex(row => row.node.entry?.id === selectedId);
    if (index < 0) index = 0;
    const row = rows[index];
    if (!row) return;
    if (event.key === 'ArrowDown') focusRow(Math.min(rows.length - 1, index + 1));
    else if (event.key === 'ArrowUp') focusRow(Math.max(0, index - 1));
    else if (event.key === 'Home') focusRow(0);
    else if (event.key === 'End') focusRow(rows.length - 1);
    else if (event.key === 'Enter') activate(row);
    else if (event.key === 'ArrowRight') {
      if (row.node.directory && !expanded.has(row.node.key)) onToggle(row.node.key);
      else if (row.node.directory) focusRow(index + 1);
    } else if (event.key === 'ArrowLeft') {
      if (row.node.directory && expanded.has(row.node.key)) onToggle(row.node.key);
      else {
        const separator = row.node.key.lastIndexOf('/');
        if (separator < 0) return;
        const parent = row.node.key.slice(0, separator);
        const parentIndex = rows.findIndex(item => item.node.key === parent);
        if (parentIndex >= 0) focusRow(parentIndex, false);
      }
    }
  }
  function nodeTitle(node: ChangeNode): string {
    const identity = node.path.includes('�') ? `\nRuta nativa: ${node.key}` : '';
    const repository = node.repository ? `\nRepositorio · ${node.repository.branch || 'Sin commits'}` : '';
    return `${node.entry?.absolutePath ?? node.repository?.path ?? node.path}${identity}${repository}`;
  }
  export function focusSelected() {
    const index = rows.findIndex(row => row.node.entry?.id === selectedId);
    if (index >= 0) focusRow(index, false);
    scroller?.focus();
  }
</script>

<div class="change-tree" bind:this={scroller} bind:clientHeight={height} onscroll={() => scrollTop = scroller.scrollTop} onkeydown={keydown} tabindex="0" role="tree" aria-label="Archivos con cambios" aria-activedescendant={focusKey ? 'node-' + encodeURIComponent(focusKey) : undefined}>
  <div class="tree-spacer" style:height={`${rows.length * rowHeight}px`}>
    <div class="tree-window" style:transform={`translateY(${start * rowHeight}px)`}>
      {#each visibleRows as row (row.node.key)}
        {@const status = row.node.entry ? statusLabel(row.node.entry, mode) : null}
        <div class="tree-row" class:repo-group={row.node.repositoryRoot} class:selected={row.node.entry?.id === selectedId} class:keyboard-focus={focusKey === row.node.key} class:directory={row.node.directory} role="treeitem" tabindex="-1" id={'node-' + encodeURIComponent(row.node.key)} aria-label={row.node.name} aria-level={row.depth + 1} aria-expanded={row.node.directory ? expanded.has(row.node.key) || !!query.trim() : undefined} aria-selected={row.node.entry ? row.node.entry.id === selectedId : undefined} title={nodeTitle(row.node)} oncontextmenu={event => { if (row.node.repository) { event.preventDefault(); focusKey = row.node.key; onRepositoryMenu(row.node.repository, event.clientX, event.clientY); } }}>
          <button type="button" class="tree-row-main" tabindex="-1" style:padding-left={`${12 + row.depth * 15}px`} onclick={() => activate(row)}>
          <span class="tree-disclosure">{#if row.node.directory}<Icon name={expanded.has(row.node.key) || query.trim() ? 'chevron-down' : 'chevron-right'} size={12} />{/if}</span>
          <span class="tree-file-icon" class:repo-icon={!!row.node.repository}><Icon name={row.node.repository ? 'repo' : row.node.directory ? 'folder' : 'file'} size={15} /></span>
          <span class="tree-name">{row.node.name}</span>
          {#if row.node.repository}<span class="repo-tag" class:repo-branch={row.node.repositoryRoot}>{row.node.repositoryRoot ? row.node.repository.branch : 'git'}</span>{/if}
          {#if status}<span class="file-status" class:status-modified={status === 'M' || status === 'T'} class:status-added={status === 'A' || status === '?'} class:status-deleted={status === 'D'} class:status-conflict={status === 'U'} class:status-renamed={status === 'R'}>{status}</span>
          {:else if row.node.changeCount}<span class="folder-count">{row.node.changeCount}</span>{/if}
          </button>
          {#if row.node.repository}<RepoUntrackedToggle repository={row.node.repository} show={visibility[row.node.repository.id] !== false} onToggle={onVisibilityChange} />{/if}
        </div>
      {/each}
    </div>
  </div>
</div>
