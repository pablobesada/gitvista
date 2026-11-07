<script lang="ts">
  import { listBranches, isDesktop } from '../lib/bridge';
  import type { Repository } from '../lib/types';

  let { repositories, generation, disabled, onChange }: {
    repositories: Repository[];
    generation: number;
    disabled: boolean;
    onChange: (repoId: string, reference: string | null) => Promise<void>;
  } = $props();
  let repoId = $state('');
  const repository = $derived(repositories.find(repo => repo.id === repoId) ?? repositories[0]);
  let branches = $state<string[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let reload = $state(0);
  const label = (reference: string) => reference.replace(/^refs\/heads\//, '').replace(/^refs\/remotes\//, 'remota: ');
  $effect(() => {
    const id = repository?.id;
    const revision = generation;
    reload;
    let cancelled = false;
    branches = [];
    error = null;
    if (!id) return;
    loading = true;
    listBranches(id, revision).then(result => { if (!cancelled) branches = result; })
      .catch(cause => { if (!cancelled) error = String(cause); })
      .finally(() => { if (!cancelled) loading = false; });
    return () => { cancelled = true; };
  });
  async function change(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const reference = select.value || null;
    // A denied draft-discard must leave the selected option unchanged.
    select.value = repository?.comparison?.reference ?? '';
    if (repository) await onChange(repository.id, reference);
  }
</script>

<div class="comparison-picker">
  <span class="comparison-caption">ARCHIVOS ACTUALES VS.</span>
  {#if repositories.length > 1}
    <select aria-label="Repositorio para comparar" value={repository?.id ?? ''} disabled={disabled} onchange={event => repoId = event.currentTarget.value}>
      {#each repositories as repo}<option value={repo.id}>{repo.relativePath || repo.path.split('/').pop()}</option>{/each}
    </select>
  {/if}
  <select aria-label="Versión de referencia" value={repository?.comparison?.reference ?? ''} disabled={disabled || !repository || !isDesktop} onchange={change} title="Comparar sin cambiar de rama">
    <option value="">Último commit (HEAD)</option>
    {#if repository?.comparison && !branches.includes(repository.comparison.reference)}<option value={repository.comparison.reference}>{label(repository.comparison.reference)}</option>{/if}
    <optgroup label={loading ? 'Cargando ramas…' : 'Ramas'}>
      {#each branches as reference}<option value={reference}>{label(reference)}</option>{/each}
    </optgroup>
  </select>
  {#if error}<div class="comparison-error">{error} <button onclick={() => reload++}>Reintentar</button></div>{/if}
  {#if !isDesktop}<small>Selección de ramas disponible en escritorio.</small>{/if}
</div>

<style>
  .comparison-picker { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px 12px; border-bottom: 1px solid var(--line); }
  .comparison-caption { font-size: 10px; letter-spacing: .06em; opacity: .65; }
  select { width: 100%; min-width: 0; padding: 5px 7px; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: inherit; font: inherit; font-size: 12px; }
  .comparison-error, small { font-size: 11px; overflow-wrap: anywhere; }
</style>
