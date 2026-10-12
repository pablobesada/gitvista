<script lang="ts">
  import type { Repository } from '../lib/types';
  import Icon from './Icon.svelte';
  let { repository, show, onToggle }: {
    repository: Repository; show: boolean;
    onToggle: (repoId: string, show: boolean) => void;
  } = $props();
  const name = $derived(repository.path.split('/').filter(Boolean).pop() ?? 'repositorio');
  const action = $derived(`${show ? 'Ocultar' : 'Mostrar'} archivos sin seguimiento de ${name}`);
</script>

<button type="button" class="repo-eye" class:off={!show} aria-label={action} aria-pressed={show} title={`${action}\n${repository.path}`} onclick={() => onToggle(repository.id, !show)}>
  <Icon name={show ? 'eye' : 'eye-off'} size={14} />
</button>

<style>
  .repo-eye { width: 22px; height: 22px; padding: 0; display: grid; place-items: center; flex-shrink: 0; color: #737f95; border-radius: 4px; }
  .repo-eye:hover { background: #ffffff09; color: #c5cfeb; }
  .repo-eye.off { color: #a5afd0; }
</style>
