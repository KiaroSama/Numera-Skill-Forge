import { bridgeAvailable, diagnostic, dispatch, object, previewAllowsApply, renderResult, resultStatus, snapshotDigest } from './bridge';
import type { Json, Request } from './bridge';

const views = ['dashboard', 'sources', 'catalog', 'installed', 'updates', 'harnesses', 'history', 'logs', 'settings'] as const;
type View = typeof views[number];
type Reviewed = { root: string; source: string; digest: string };
let workspace = '';
let busy = false;
let activeJob: string | undefined;
let reviewed: Reviewed | undefined;
let installPreview: string | undefined;

function element<T extends Element>(selector: string, constructor: { new (...args: never[]): T }): T {
  const found = document.querySelector(selector);
  if (!(found instanceof constructor)) throw new Error(`Missing interface element: ${selector}`);
  return found;
}
const rootInput = element('#workspace-root', HTMLInputElement);
const workspaceForm = element('#workspace-form', HTMLFormElement);
const approvalForm = element('#approval-form', HTMLFormElement);
const installForm = element('#install-form', HTMLFormElement);
const theme = element('#theme', HTMLSelectElement);
const notice = element('#notice', HTMLDivElement);
const bridgeState = element('#bridge-state', HTMLParagraphElement);
const main = element('#main', HTMLElement);

function preference(key: string): string {
  try { return localStorage.getItem(key) ?? ''; } catch { return ''; }
}
function savePreference(key: string, value: string): void {
  try { localStorage.setItem(key, value); } catch {
    tell('Local preferences could not be saved. The selected workspace remains open for this session.', true);
  }
}
function tell(message: string, error = false): void {
  notice.textContent = message;
  notice.classList.toggle('error', error);
  notice.setAttribute('role', error ? 'alert' : 'status');
}
function activeView(): View {
  const id = location.hash.slice(1);
  return views.find(view => view === id) ?? 'dashboard';
}
function navigate(focus: boolean): void {
  const id = activeView();
  for (const view of views) {
    element(`#${view}`, HTMLElement).hidden = view !== id;
    const link = element(`[data-view="${view}"]`, HTMLAnchorElement);
    if (view === id) link.setAttribute('aria-current', 'page');
    else link.removeAttribute('aria-current');
  }
  document.title = `${id[0]?.toUpperCase()}${id.slice(1)} · Numera Skill Forge`;
  if (focus) element(`#${id}-title`, HTMLHeadingElement).focus();
}
function input(form: HTMLFormElement, name: string): HTMLInputElement {
  const field = form.elements.namedItem(name);
  if (!(field instanceof HTMLInputElement)) throw new Error(`Missing input: ${name}`);
  return field;
}
function text(form: HTMLFormElement, name: string): string {
  const data = new FormData(form).get(name);
  return typeof data === 'string' ? data.trim() : '';
}
function list(form: HTMLFormElement, name: string): string[] {
  return [...new Set(text(form, name).split(/[\n,]/).map(value => value.trim()).filter(Boolean))];
}
function checked(form: HTMLFormElement, name: string): boolean {
  return input(form, name).checked;
}
function showResult(view: View, result: Json): void {
  renderResult(element(`[data-result="${view}"]`, HTMLElement), result);
}
function clearApproval(): void {
  reviewed = undefined;
  approvalForm.reset();
}
function clearPreview(): void {
  installPreview = undefined;
  input(installForm, 'confirmed').checked = false;
  element('#install-gate', HTMLParagraphElement).textContent = 'A matching real dry-run response is required before applying.';
  updateControls();
}
function installKey(): string {
  return JSON.stringify([workspace, text(installForm, 'package'), text(installForm, 'target'), text(installForm, 'destination'), text(installForm, 'scope')]);
}
function updateControls(): void {
  const unavailable = !bridgeAvailable() || !workspace || busy;
  for (const fieldset of document.querySelectorAll<HTMLFieldSetElement>('.operation-fields')) fieldset.disabled = unavailable;
  for (const button of document.querySelectorAll<HTMLButtonElement>('[data-action]')) button.disabled = unavailable;
  for (const field of workspaceForm.querySelectorAll<HTMLInputElement | HTMLButtonElement>('input, button')) field.disabled = busy;
  const apply = element('[data-intent="apply-install"]', HTMLButtonElement);
  apply.disabled = unavailable || installPreview !== installKey() || !checked(installForm, 'confirmed');
  const approve = element('#approval-form button', HTMLButtonElement);
  approve.disabled = unavailable || reviewed === undefined || !checked(approvalForm, 'reviewed');
  main.setAttribute('aria-busy', String(busy));
}
function options(id: string, result: Json, key: string): void {
  if (!Array.isArray(result)) return;
  const datalist = element(`#${id}`, HTMLDataListElement);
  datalist.replaceChildren();
  for (const row of result) {
    if (!object(row) || typeof row[key] !== 'string') continue;
    const option = document.createElement('option');
    option.value = row[key];
    datalist.append(option);
  }
}
function rememberOptions(action: string, result: Json): void {
  if (action === 'source-list') options('source-ids', result, 'id');
  if (action === 'catalog') options('package-ids', result, 'id');
  if (action === 'harnesses') options('target-profiles', result, 'harness');
  if (action === 'installed') options('receipt-ids', result, 'id');
  if (action === 'history') options('operation-ids', result, 'id');
}
async function run(request: Request, view: View, form?: HTMLFormElement): Promise<Json | undefined> {
  if (busy) return undefined;
  const errorField = form?.querySelector<HTMLElement>('.form-error');
  if (errorField) errorField.textContent = '';
  busy = true;
  updateControls();
  tell(`Running ${request.action}… The service owns validation and transaction safety.`);
  try {
    const jobEnabled = ['sync', 'discover', 'build', 'collection-sync', 'update'].includes(request.action);
    activeJob = jobEnabled ? `ui-${crypto.randomUUID()}` : undefined;
    element('#cancel-job', HTMLButtonElement).hidden = activeJob === undefined;
    const response = await dispatch(workspace, activeJob ? { action: 'job-run', job: activeJob, timeout: 600, request } : request);
    if (object(response) && (response['status'] === 'failed' || response['status'] === 'cancelled')) throw new Error(JSON.stringify(response['error']));
    const result = object(response) && 'result' in response ? response['result'] as Json : response;
    showResult(view, result);
    rememberOptions(request.action, result);
    tell(`${request.action}: ${resultStatus(result)}. Review the full response below.`);
    return result;
  } catch (error: unknown) {
    const message = diagnostic(error);
    if (errorField) errorField.textContent = message;
    const failure = document.createElement('p');
    failure.className = 'error';
    failure.textContent = message;
    element(`[data-result="${view}"]`, HTMLElement).replaceChildren(failure);
    tell(message, true);
    return undefined;
  } finally {
    activeJob = undefined;
    element('#cancel-job', HTMLButtonElement).hidden = true;
    busy = false;
    updateControls();
  }
}
element('#cancel-job', HTMLButtonElement).addEventListener('click', () => {
  if (!activeJob) return;
  void dispatch(workspace, { action: 'job-cancel', job: activeJob }).then(() => tell('Cancellation requested; waiting for a safe operation boundary.')).catch(error => tell(diagnostic(error), true));
});
async function openWorkspace(root: string): Promise<void> {
  if (busy) return;
  workspace = root.trim();
  clearApproval();
  clearPreview();
  for (const result of document.querySelectorAll<HTMLElement>('[data-result]')) {
    const empty = document.createElement('p');
    empty.className = 'empty';
    empty.textContent = 'No data loaded for this workspace.';
    result.replaceChildren(empty);
  }
  for (const datalist of document.querySelectorAll('datalist')) datalist.replaceChildren();
  busy = true;
  updateControls();
  tell('Opening workspace and checking enabled sources sequentially…');
  try {
    const status = await dispatch(workspace, { action: 'status' });
    showResult('dashboard', status);
    savePreference('numera-workspace', workspace);
    const sync = await dispatch(workspace, { action: 'sync', apply: false, offline: false });
    showResult('updates', sync);
    for (const [action, view] of [['source-list', 'sources'], ['catalog', 'catalog'], ['harnesses', 'harnesses']] as const) {
      try {
        const result = await dispatch(workspace, { action });
        rememberOptions(action, result);
        showResult(view, result);
      } catch (error: unknown) {
        const failure = document.createElement('p');
        failure.className = 'error';
        failure.textContent = diagnostic(error);
        element(`[data-result="${view}"]`, HTMLElement).replaceChildren(failure);
      }
    }
    tell(`Workspace opened. Sequential check: ${resultStatus(sync)}. Review Updates for details.`);
  } catch (error: unknown) {
    tell(diagnostic(error), true);
  } finally {
    busy = false;
    updateControls();
  }
}

workspaceForm.addEventListener('submit', event => {
  event.preventDefault();
  if (workspaceForm.reportValidity()) void openWorkspace(rootInput.value);
});
for (const button of document.querySelectorAll<HTMLButtonElement>('[data-action]')) {
  button.addEventListener('click', () => {
    const action = button.dataset['action'];
    if (action) void run({ action }, activeView());
  });
}

async function submit(form: HTMLFormElement, submitter: HTMLElement | null): Promise<void> {
  const operation = form.dataset['operation'];
  const intent = submitter?.dataset['intent'];
  const view = activeView();
  if (!operation || busy || !form.reportValidity()) return;
  try {
    let request: Request;
    switch (operation) {
      case 'source-add':
        clearApproval();
        request = { action: operation, id: text(form, 'id'), repository: text(form, 'repository'), roots: list(form, 'roots'), targets: list(form, 'targets'), enabled: checked(form, 'enabled') };
        break;
      case 'discover':
        clearApproval();
        request = { action: operation, source: text(form, 'source'), offline: checked(form, 'offline') };
        break;
      case 'approve':
        if (!reviewed || reviewed.root !== workspace || reviewed.source !== text(form, 'source') || reviewed.digest !== text(form, 'digest') || !checked(form, 'reviewed')) {
          throw new Error('Discover and review this workspace candidate again before approving.');
        }
        request = { action: operation, source: reviewed.source, digest: reviewed.digest, allow_executable: checked(form, 'allow_executable') };
        break;
      case 'package-add':
        request = { action: operation, package: { id: text(form, 'id'), name: text(form, 'name'), source: text(form, 'source'), components: list(form, 'components'), dependencies: list(form, 'dependencies'), targets: list(form, 'targets'), optional_exclusions: list(form, 'optional_exclusions') } };
        clearPreview();
        break;
      case 'build':
      case 'compatibility':
        request = { action: intent === 'compatibility' ? 'compatibility' : operation, package: text(form, 'package'), target: text(form, 'target') };
        if (request.action === 'build') clearPreview();
        break;
      case 'install': {
        const apply = intent === 'apply-install';
        if (apply && (installPreview !== installKey() || !checked(form, 'confirmed'))) throw new Error('Preview these exact destination changes and authorize them before applying.');
        request = { action: operation, package: text(form, 'package'), target: text(form, 'target'), destination: text(form, 'destination'), scope: text(form, 'scope'), dry_run: !apply };
        clearPreview();
        break;
      }
      case 'receipt':
        clearPreview();
        request = { action: text(form, 'operation'), receipt: text(form, 'receipt') };
        break;
      case 'collection-sync':
      case 'sync':
        clearApproval();
        clearPreview();
        request = { action: operation, apply: checked(form, 'apply'), offline: checked(form, 'offline') };
        break;
      case 'rollback':
        clearPreview();
        request = { action: operation, operation: text(form, 'operation') };
        break;
      case 'job':
        request = { action: text(form, 'operation'), job: text(form, 'job') };
        break;
      case 'support-bundle':
        request = { action: operation, include_logs: checked(form, 'include_logs') };
        break;
      case 'log-retention':
        request = { action: operation, days: Number(text(form, 'days')), keep: Number(text(form, 'keep')), apply: checked(form, 'apply') };
        break;
      case 'publish': {
        const mode = text(form, 'mode');
        if (mode !== 'preview' && !checked(form, 'confirmed')) throw new Error('Explicit repository and publication review is required before mutation.');
        request = { action: operation, mode };
        for (const field of ['expected_remote', 'expected_head', 'expected_remote_head', 'expected_account', 'publication_branch', 'author_name', 'author_email']) {
          const value = text(form, field);
          if (value) request[field] = value;
        }
        break;
      }
      default: throw new Error('This form has no supported service operation.');
    }
    const result = await run(request, view, form);
    if (result === undefined) return;
    if (request.action === 'discover' && typeof request['source'] === 'string') {
      const digest = snapshotDigest(result, request['source']);
      if (digest) {
        reviewed = { root: workspace, source: request['source'], digest };
        input(approvalForm, 'source').value = reviewed.source;
        input(approvalForm, 'digest').value = digest;
      } else tell('No matching content digest was returned. Approval remains locked; inspect the discovery diagnostic.', true);
    }
    if (request.action === 'approve') clearApproval();
    if (request.action === 'install' && request['dry_run'] === true && typeof request['destination'] === 'string' && previewAllowsApply(result, request['destination'])) {
      installPreview = installKey();
      element('#install-gate', HTMLParagraphElement).textContent = 'Preview received. Inspect the destination and files above, then explicitly authorize the changes.';
    }
    if (request.action === 'install' && request['dry_run'] === false) clearPreview();
    updateControls();
  } catch (error: unknown) {
    const message = diagnostic(error);
    const field = form.querySelector<HTMLElement>('.form-error');
    if (field) field.textContent = message;
    tell(message, true);
  }
}
for (const form of document.querySelectorAll<HTMLFormElement>('form[data-operation]')) {
  form.addEventListener('submit', event => {
    event.preventDefault();
    void submit(form, event instanceof SubmitEvent && event.submitter instanceof HTMLElement ? event.submitter : null);
  });
}
installForm.addEventListener('input', event => {
  if (!(event.target instanceof HTMLInputElement) || event.target.name !== 'confirmed') clearPreview();
  else updateControls();
});
approvalForm.addEventListener('input', updateControls);
window.addEventListener('hashchange', () => navigate(true));
theme.value = ['light', 'dark'].includes(preference('numera-theme')) ? preference('numera-theme') : 'system';
function applyTheme(): void {
  document.documentElement.dataset['theme'] = theme.value;
}
theme.addEventListener('change', () => { applyTheme(); savePreference('numera-theme', theme.value); });
applyTheme();
navigate(false);
bridgeState.textContent = bridgeAvailable() ? 'Native desktop bridge connected · Local operations only' : 'Desktop bridge unavailable — browser mode is read-only. Open the native app for real workspace operations.';
bridgeState.classList.toggle('unavailable', !bridgeAvailable());
rootInput.value = preference('numera-workspace');
updateControls();
if (bridgeAvailable() && rootInput.value.trim()) void openWorkspace(rootInput.value);
