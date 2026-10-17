<script lang="ts">
  import { onMount } from 'svelte';
  import type { Repository } from '../lib/types';
  import Icon from './Icon.svelte';
  let { repository, x, y, onClose, onCleanup }: { repository: Repository; x: number; y: number; onClose: () => void; onCleanup: (repository: Repository) => void } = $props();
  let menu: HTMLDivElement;
  let item: HTMLButtonElement;
  let left = $state(0);
  let top = $state(0);
  onMount(() => {
    const prior = document.activeElement as HTMLElement | null;
    left = Math.max(6, Math.min(x, window.innerWidth - menu.offsetWidth - 6));
    top = Math.max(6, Math.min(y, window.innerHeight - menu.offsetHeight - 6));
    item.focus();
    return () => { if (prior?.isConnected) prior.focus({ preventScroll: true }); };
  });
  function keydown(event: KeyboardEvent) {
    event.stopPropagation();
    if (['Escape', 'Tab'].includes(event.key)) { event.preventDefault(); onClose(); }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) { event.preventDefault(); item.focus(); }
  }
</script>

<svelte:window onresize={onClose} onblur={onClose} />
<button class="repository-menu-backdrop" aria-label="Cerrar menú del repositorio" tabindex="-1" onclick={onClose} oncontextmenu={event => { event.preventDefault(); onClose(); }}></button>
<div class="repository-menu" bind:this={menu} role="menu" aria-label={`Acciones de ${repository.path.split('/').pop()}`} tabindex="-1" style:left={`${left}px`} style:top={`${top}px`} onkeydown={keydown}>
  <button bind:this={item} role="menuitem" onclick={() => onCleanup(repository)}><Icon name="restore" size={15} /><span>Guardar en stash y limpiar…</span></button>
</div>

<style>
  .repository-menu-backdrop { position: fixed; inset: 0; z-index: 80; background: transparent; cursor: default; }
  .repository-menu { position: fixed; z-index: 81; padding: 5px; min-width: 235px; border: 1px solid #ffffff18; border-radius: 9px; background: #24272f; box-shadow: 0 12px 40px #0006; }
  .repository-menu button { display: flex; gap: 10px; align-items: center; width: 100%; padding: 10px; text-align: left; border-radius: 5px; color: #d2d9e8; font-size: 12px; }
  .repository-menu button:hover, .repository-menu button:focus-visible { background: #a7b0f91c; outline: none; }
</style>
