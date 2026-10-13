<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import { badgeDescription, rootLabel } from '../lib/workspace-tabs';
  import type { RootBadgeState } from '../lib/types';

  let { roots, activeRoot, badges, onSelect, onClose }: {
    roots: string[]; activeRoot: string | null;
    badges: Record<string, RootBadgeState>;
    onSelect: (root: string) => void; onClose: (root: string) => void;
  } = $props();
  let strip = $state<HTMLDivElement>();

  function navigate(event: KeyboardEvent, index: number) {
    let next = index;
    if (event.key === 'ArrowRight') next = (index + 1) % roots.length;
    else if (event.key === 'ArrowLeft') next = (index + roots.length - 1) % roots.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = roots.length - 1;
    else return;
    event.preventDefault();
    onSelect(roots[next]);
    strip?.querySelectorAll<HTMLButtonElement>('[role="tab"]')[next]?.focus();
  }
  $effect(() => {
    activeRoot; roots.length;
    untrack(() => { void tick().then(() => strip?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' })); });
  });
</script>

<div class="root-tabs" role="tablist" aria-label="Carpetas raíz abiertas" bind:this={strip}>
  {#each roots as root, index (root)}
    <div class="root-tab" class:active={root === activeRoot}>
      <button class="root-tab-select" role="tab" aria-selected={root === activeRoot} aria-label={`${rootLabel(root)} · ${badgeDescription(badges[root], root === activeRoot)}`} aria-controls="workspace-panel" tabindex={root === activeRoot ? 0 : -1} title={`${root}\n${badgeDescription(badges[root], root === activeRoot)}`} onclick={() => onSelect(root)} onkeydown={event => navigate(event, index)}>
        <Icon name={root === activeRoot ? 'folder-open' : 'folder'} size={14} /><span>{rootLabel(root)}</span>
        {#if badges[root]?.hasChanges || !badges[root] || badges[root].hasChanges === null || badges[root].stale}
          <i class="root-change-dot" class:changed={badges[root]?.hasChanges === true} class:uncertain={badges[root]?.stale || badges[root]?.hasChanges == null} aria-hidden="true"></i>
        {/if}
      </button>
      <button class="root-tab-close" aria-label={`Cerrar raíz ${rootLabel(root)}`} title={`Cerrar ${root}`} onclick={() => onClose(root)}><Icon name="close" size={12} /></button>
    </div>
  {/each}
</div>
