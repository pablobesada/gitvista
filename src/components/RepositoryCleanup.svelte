<script lang="ts">
  import { onMount } from 'svelte';
  import { copyText, previewRepositoryCleanup, stashRepository } from '../lib/bridge';
  import type { Repository, StashPreview, StashResult } from '../lib/types';
  import Icon from './Icon.svelte';
  let { repository, generation, onBusy, onClose }: { repository: Repository; generation: number; onBusy: (busy: boolean) => void; onClose: () => void } = $props();
  let dialog: HTMLDialogElement;
  let preview = $state<StashPreview | null>(null);
  let result = $state<StashResult | null>(null);
  let loading = $state(true);
  let busy = $state(false);
  let includeUntracked = $state(true);
  let error = $state<string | null>(null);
  let copied = $state(false);
  const name = $derived(repository.path.split('/').filter(Boolean).pop());
  const canClean = $derived(preview && (preview.tracked > 0 || (includeUntracked && preview.untracked > 0)));
  // Only copied as text for the user's terminal; never executed by the app.
  const recoveryCommand = $derived(result?.stashOid ? `git stash apply --index ${result.stashOid}` : '');
  onMount(() => {
    const prior = document.activeElement as HTMLElement | null;
    dialog.showModal();
    void load();
    return () => { if (prior?.isConnected) prior.focus({ preventScroll: true }); };
  });
  async function load() {
    loading = true; error = null;
    try { preview = await previewRepositoryCleanup(repository.id, generation); }
    catch (cause) { error = String(cause); }
    finally { loading = false; }
  }
  async function clean() {
    if (!preview || busy) return;
    busy = true; onBusy(true); error = null;
    try { result = await stashRepository(repository.id, generation, preview.token, includeUntracked); }
    catch (cause) { error = String(cause); }
    finally { busy = false; onBusy(false); }
  }
  async function copyRecovery() {
    try { await copyText(recoveryCommand); copied = true; }
    catch (cause) { error = String(cause); }
  }
</script>

<dialog bind:this={dialog} aria-labelledby="cleanup-title" aria-describedby="cleanup-description" oncancel={event => { event.preventDefault(); if (!busy) onClose(); }} onkeydown={event => event.stopPropagation()}>
  <div class="dialog-icon"><Icon name={result?.warning || error ? 'warning' : result ? 'check' : 'restore'} size={22} /></div>
  <h2 id="cleanup-title">{result ? result.warning ? 'Revisá el resultado' : 'Cambios guardados' : 'Guardar en stash y limpiar'}</h2>
  <strong>{name}</strong><div class="repo-path">{repository.path}</div>
  <p id="cleanup-description">{result ? result.stashOid ? 'El respaldo quedó guardado en el stash de este repositorio.' : 'No se pudo confirmar un respaldo nuevo.' : 'Se guardarán los cambios y el repositorio volverá a HEAD, incluyendo el índice. Podés recuperarlos después desde Git.'}</p>
  {#if loading}<p role="status">Consultando el repositorio…</p>
  {:else if preview && !result}
    <div class="counts"><span><b>{preview.tracked}</b> archivos versionados</span><small>{preview.staged} preparados · {preview.unstaged} sin preparar</small></div>
    <label><input type="checkbox" bind:checked={includeUntracked} disabled={busy} />Incluir archivos sin seguimiento <span>{preview.untracked}</span></label>
    <p class="scope-note">Sólo este repositorio. Los archivos ignorados y el contenido de los repositorios anidados se conservan.</p>
    {#if !canClean}<p>No hay cambios para guardar con estas opciones.</p>{/if}
  {/if}
  {#if result?.stashOid}<code class="stash-id">Stash · {result.stashOid.slice(0, 12)}</code><p class="scope-note">Desde una terminal ubicada en este repositorio, ejecutá este comando para recuperar los cambios. El stash se conserva al aplicarlo.</p><textarea readonly aria-label="Comando para recuperar el stash" value={recoveryCommand} rows="3"></textarea><button class="copy-action" onclick={copyRecovery}><Icon name="copy" size={13} />{copied ? 'Comando copiado' : 'Copiar comando de recuperación'}</button>{/if}
  {#if result?.warning}<p class="error" role="alert">{result.warning}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="actions">
    <button disabled={busy} onclick={onClose}>{result ? 'Cerrar' : 'Cancelar'}</button>
    {#if !result}<button class="confirm" disabled={loading || busy || !canClean || !!error} onclick={clean}>{busy ? 'Guardando y limpiando…' : 'Guardar y limpiar'}</button>{/if}
  </div>
</dialog>

<style>
  dialog { width: min(460px, 90vw); max-height: 85vh; overflow: auto; color: #d8dfed; padding: 26px; border: 1px solid #ffffff1a; border-radius: 14px; background: #22262e; box-shadow: 0 24px 80px #0008; }
  dialog::backdrop { background: #07090e99; backdrop-filter: blur(3px); }
  .dialog-icon { color: #adb7ef; margin-bottom: 15px; }
  h2 { margin: 0 0 19px; font-size: 18px; font-weight: 600; }
  strong { font-size: 13px; }
  p { line-height: 1.7; color: #a0acc0; font-size: 12px; }
  .repo-path { color: #8894aa; font-size: 11px; margin: 6px 0 16px; overflow-wrap: anywhere; }
  .counts { display: flex; flex-direction: column; gap: 7px; background: #10131866; border-radius: 7px; padding: 13px; margin: 16px 0; font-size: 12px; }
  small, .scope-note { color: #8c98ac; font-size: 11px; }
  label { display: flex; align-items: center; gap: 9px; font-size: 12px; padding: 6px 0; }
  label span { margin-left: auto; color: #a3afd0; }
  input { accent-color: #a7b0f9; }
  .actions { display: flex; justify-content: flex-end; gap: 9px; margin-top: 24px; }
  button { display: inline-flex; align-items: center; gap: 6px; padding: 9px 12px; border-radius: 6px; background: #ffffff09; font-size: 12px; }
  button:hover:not(:disabled) { background: #ffffff14; }
  button.confirm { background: #a7b0f929; color: #d3d9ff; }
  button:disabled { opacity: .45; cursor: default; }
  .copy-action { margin-top: 8px; font-size: 11px; }
  .stash-id { display: block; color: #a5c6ac; font-size: 12px; }
  textarea { resize: none; width: 100%; box-sizing: border-box; color: #c9d4ea; background: #111419; border: 1px solid #ffffff0b; border-radius: 6px; padding: 10px; font-family: monospace; font-size: 11px; }
  .error { white-space: pre-wrap; overflow-wrap: anywhere; color: #e7b2ab; padding: 10px; background: #d78a7910; border-radius: 6px; }
</style>
