import { invoke, isTauri } from '@tauri-apps/api/core';

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type Request = { action: string; [key: string]: Json };

export function object(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function isJson(value: unknown): value is Json {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return true;
  if (typeof value === 'number') return Number.isFinite(value);
  if (Array.isArray(value)) return value.every(isJson);
  return object(value) && Object.values(value).every(isJson);
}

export function bridgeAvailable(): boolean {
  return isTauri();
}

export async function dispatch(root: string, request: Request): Promise<Json> {
  if (!bridgeAvailable()) {
    throw new Error('Desktop bridge unavailable. Open the native Numera Skill Forge app; browser preview cannot access a workspace.');
  }
  if (!root.trim()) throw new Error('Select a workspace before running an operation.');
  const result: unknown = await invoke('dispatch', { root, request });
  if (!isJson(result)) throw new Error('The desktop service returned a non-JSON response. Review the diagnostic log.');
  return result;
}

export function diagnostic(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  if (object(error) && typeof error['message'] === 'string') return error['message'];
  return 'The operation failed without a readable diagnostic. Review the desktop log.';
}

export function resultStatus(result: Json): string {
  return object(result) && typeof result['status'] === 'string'
    ? result['status'] : 'Response received — inspect the result';
}

export function snapshotDigest(result: Json, source: string): string | undefined {
  if (!object(result)) return undefined;
  const snapshot = object(result['snapshot']) ? result['snapshot'] : result;
  if (snapshot['source'] !== source) return undefined;
  const digest = snapshot['content_digest'];
  return typeof digest === 'string' && /^[a-f0-9]{64}$/i.test(digest) ? digest : undefined;
}

export function previewAllowsApply(result: Json, destination: string): boolean {
  return object(result) && result['dry_run'] === true && result['destination'] === destination
    && Array.isArray(result['files']) && result['files'].every(value => typeof value === 'string')
    && typeof result['changes'] === 'number' && Number.isSafeInteger(result['changes']) && result['changes'] >= 0
    && (result['status'] === undefined || result['status'] === 'success');
}

export function renderResult(container: HTMLElement, result: Json): void {
  container.replaceChildren();
  const status = document.createElement('p');
  status.className = 'result-status';
  status.textContent = resultStatus(result);
  const pre = document.createElement('pre');
  pre.tabIndex = 0;
  pre.setAttribute('aria-label', 'Service response JSON');
  pre.textContent = JSON.stringify(result, null, 2);
  container.append(status, pre);
}
