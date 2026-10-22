<script lang="ts">
  // Earlier versions of the open config, kept each time it was saved over (the newest first).
  // Each shows what restoring it would change; restoring makes it the unsaved draft, which
  // Save keeps and Ctrl+Z takes back.
  import { ChevronDown, History } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { language, t } from '../lib/i18n/index.svelte';
  import { isSame, profileChanges, type Changes } from '../lib/model/changes';
  import { comboLabel } from '../lib/model/labels';
  import { app } from '../lib/state/app.svelte';
  import type { Profile } from '../lib/types';
  import Dialog from './ui/Dialog.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let versions = $state<{ path: string; time_ms: number }[] | null>(null);
  /** The version whose changes show, with its profile. */
  let open = $state<{ path: string; profile: Profile; changes: Changes } | null>(null);

  // svelte-ignore state_referenced_locally
  if (app.configPath) app.attempt(api.configVersions(app.configPath).then((v) => (versions = v)));

  const when = (ms: number) => new Date(ms).toLocaleString(language(), { dateStyle: 'medium', timeStyle: 'short' });

  async function show(path: string) {
    if (open?.path === path) {
      open = null;
      return;
    }
    const loaded = await api.configLoad(path).catch(() => null);
    if (!loaded || !app.profile) return;
    open = { path, profile: loaded.profile, changes: profileChanges($state.snapshot(app.profile) as Profile, loaded.profile) };
  }

  function restore(v: { path: string; time_ms: number }) {
    if (!open || open.path !== v.path) return;
    app.update(open.profile);
    app.toast(t('history.restored', { when: when(v.time_ms) }), 'ok');
    onclose();
  }
</script>

<Dialog title={t('history.title')} {onclose} width={560} gap={12}>
  {#snippet icon()}<History size={17} />{/snippet}
  <p class="hint">{t('history.hint')}</p>
  {#if versions === null}
    <p class="hint">…</p>
  {:else if versions.length === 0}
    <p class="hint">{t('history.none')}</p>
  {:else}
    <div class="list">
      {#each versions as v (v.path)}
        <div class="version">
          <button class="row" onclick={() => show(v.path)} aria-expanded={open?.path === v.path}>
            <span>{when(v.time_ms)}</span>
            <ChevronDown size={14} class={open?.path === v.path ? 'flip' : ''} />
          </button>
          {#if open?.path === v.path}
            <div class="changes">
              {#if isSame(open.changes)}
                <p class="hint">{t('history.same')}</p>
              {:else}
                <ul>
                  {#each open.changes.mappings as c, i (i)}
                    <li>
                      <span class="kind {c.kind}">{t(`history.${c.kind}`)}</span>
                      {comboLabel(c.trigger, app.family)}
                      <span class="dim"
                        >· {app.layerLabel(c.layer)}{c.activator !== 'single' ? ` · ${t(`activators.${c.activator}`)}` : ''}</span
                      >
                    </li>
                  {/each}
                  {#each open.changes.sections as s (s)}
                    <li><span class="kind changed">{t('history.changed')}</span> {t('history.section', { name: s })}</li>
                  {/each}
                </ul>
                <button class="btn primary" onclick={() => restore(v)} disabled={app.readOnly}>{t('history.restore')}</button>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</Dialog>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: min(460px, 60vh);
    overflow-y: auto;
  }
  .version {
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: 8px 10px;
    text-align: left;
  }
  .row :global(.flip) {
    transform: rotate(180deg);
  }
  .changes {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 0 10px 10px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: var(--fs-s);
  }
  .kind {
    display: inline-block;
    min-width: 72px;
    font-size: var(--fs-xs);
  }
  .kind.added {
    color: var(--ok);
  }
  .kind.removed {
    color: var(--danger);
  }
  .kind.changed {
    color: var(--warning);
  }
  .dim {
    color: var(--text-dim);
  }
</style>
