import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { Preferences, WorkspaceEvent, WorkspaceSnapshot, DiffMode, DiffResponse, DiffContents, EditVersion } from './types';
import { demoContents, demoDiff, demoWorkspace, saveDemoContents } from './demo';

export const isDesktop = isTauri();
export const defaultPreferences: Preferences = { recentRoots: [], openedRoots: [], extraRepositories: {}, sidebarWidth: 280, expandedPaths: {}, lastRoot: null, treeView: 'folders', untrackedByRepo: {} };
let demoGeneration = 0;
let demoRoot: string | null = null;
let demoListener: ((event: WorkspaceEvent) => void) | null = null;
let workspaceChannel: Channel<WorkspaceEvent> | null = null;
// Root transitions stay ordered even when the user switches tabs during IPC.
let workspaceTransition: Promise<unknown> = Promise.resolve();
function transition<T>(operation: () => Promise<T>): Promise<T> {
  const result = workspaceTransition.then(operation);
  workspaceTransition = result.catch(() => undefined);
  return result;
}

export async function getPreferences(): Promise<Preferences> {
  if (isDesktop) return invoke('get_preferences');
  try { return { ...structuredClone(defaultPreferences), ...JSON.parse(localStorage.getItem('gitvista.preferences') ?? '{}') }; }
  catch { return structuredClone(defaultPreferences); }
}
export async function savePreferences(preferences: Preferences): Promise<void> {
  if (isDesktop) return invoke('save_preferences', { preferences });
  localStorage.setItem('gitvista.preferences', JSON.stringify(preferences));
}
export async function chooseRoot(): Promise<string | null> {
  if (!isDesktop) return demoRoot === '/preview/oojs' ? '/preview/oojs3' : '/preview/oojs';
  const result = await open({ directory: true, multiple: false, title: 'Abrir carpeta de proyecto' });
  return typeof result === 'string' ? result : null;
}
export async function chooseRepository(): Promise<string | null> {
  if (!isDesktop) return null;
  const result = await open({ directory: true, multiple: false, title: 'Agregar repositorio anidado' });
  return typeof result === 'string' ? result : null;
}
export async function openWorkspace(root: string, onEvent: (event: WorkspaceEvent) => void): Promise<WorkspaceSnapshot> {
  return transition(async () => {
    if (isDesktop) {
      workspaceChannel = new Channel<WorkspaceEvent>();
      workspaceChannel.onmessage = onEvent;
      return invoke('open_workspace', { root, onEvent: workspaceChannel });
    }
    demoListener = onEvent;
    demoRoot = root;
    const snapshot = demoWorkspace(root, ++demoGeneration);
    setTimeout(() => onEvent({ type: 'reset', snapshot }), 0);
    return snapshot;
  });
}
export async function closeWorkspace(): Promise<void> {
  return transition(async () => {
    if (isDesktop) await invoke('close_workspace');
    workspaceChannel = null;
    demoListener = null;
    demoRoot = null;
    ++demoGeneration;
  });
}
export async function refreshWorkspace(): Promise<void> {
  if (isDesktop) return invoke('refresh_workspace');
  if (!demoRoot) return;
  const snapshot = demoWorkspace(demoRoot, ++demoGeneration);
  demoListener?.({ type: 'reset', snapshot });
  demoListener?.({ type: 'done', generation: demoGeneration, elapsedMs: 42 });
}
export async function addRepository(path: string): Promise<void> {
  if (isDesktop) return invoke('add_repository', { path });
}
export async function getDiff(entryId: string, mode: DiffMode, requestId: number): Promise<DiffResponse> {
  if (isDesktop) return invoke('get_diff', { entryId, mode, requestId });
  await new Promise((resolve) => setTimeout(resolve, 35));
  return { requestId, generation: demoGeneration, result: demoDiff(entryId, mode) };
}
export async function loadDiffContents(entryId: string, mode: DiffMode, requestId: number): Promise<DiffContents> {
  if (isDesktop) return invoke('load_diff_contents', { entryId, mode, requestId });
  return { ...demoContents(entryId, mode), editVersion: mode === 'staged' ? null : { generation: demoGeneration, token: 'preview' } };
}
export async function saveWorkingFile(entryId: string, version: EditVersion, contents: string): Promise<void> {
  if (isDesktop) return invoke('save_working_file', { entryId, version, contents });
  saveDemoContents(entryId, contents);
}
export async function setEditorActive(active: boolean): Promise<void> {
  if (isDesktop) await invoke('set_editor_active', { active });
}
export async function onExitRequested(handler: () => void): Promise<() => void> {
  if (isDesktop) return listen('gitvista:exit-requested', handler);
  return () => {};
}
export async function exitApplication(): Promise<void> {
  if (isDesktop) await invoke('exit_application');
}
export async function copyText(text: string): Promise<void> {
  if (isDesktop) return writeText(text);
  return navigator.clipboard.writeText(text);
}
export async function readClipboardText(): Promise<string> {
  if (isDesktop) return (await readText()) ?? '';
  return navigator.clipboard.readText();
}
