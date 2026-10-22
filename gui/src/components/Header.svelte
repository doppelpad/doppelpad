<script lang="ts">
  import { ChevronDown, Ellipsis, Save, Settings, Lock, Undo2, Redo2, Search } from '@lucide/svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { bindingsSheet } from '../lib/model/sheet';
  import { TEMPLATES } from '../lib/model/labels';
  import { app } from '../lib/state/app.svelte';
  import { platform } from '../lib/state/window.svelte';
  import { shortcutLabel, withShortcut } from '../lib/state/shortcuts';
  import type { Profile, Template } from '../lib/types';
  import Menu, { type MenuItem } from './ui/Menu.svelte';
  import WindowControls from './WindowControls.svelte';

  let { onsettings }: { onsettings: () => void } = $props();

  const configName = $derived(app.profile?.name || '—');
  const mac = platform === 'macos';

  function fromTemplate(tpl: Template) {
    if (app.gameDir) app.createConfig(api.configFromTemplate(app.gameDir, t(`welcome.templates.${tpl}`), tpl));
  }

  async function importJson() {
    if (!app.gameDir) {
      app.toast(t('header.no_selection'), 'error');
      return;
    }
    const source = await open({ title: t('header.import_title'), multiple: false, directory: false });
    if (typeof source !== 'string') return;
    await app.attempt(
      api.configImport(app.gameDir, source).then(async (r) => {
        await app.openCreated(r.path);
        app.toast(t('notices.imported', { name: app.profile?.name ?? '' }), 'ok');
        for (const d of r.diagnostics.filter((d) => d.severity !== 'info').slice(0, 3)) app.toast(d.message, 'info');
      }),
    );
  }

  async function saveSheet() {
    if (!app.profile) return;
    const path = await save({
      title: t('sheet.save_title'),
      defaultPath: `${app.profile.name || t('sheet.default_file')}.html`,
      filters: [{ name: t('sheet.filter'), extensions: ['html'] }],
    });
    if (!path) return;
    await app.attempt(api.sheetSave(path, bindingsSheet($state.snapshot(app.profile) as Profile, app.family)), t('sheet.saved'));
  }

  async function remove() {
    if (!app.configPath || !(await app.confirmAsk(t('header.delete_confirm', { name: configName })))) return;
    await app.attempt(
      api.configDelete(app.configPath).then(async () => {
        app.closeConfig();
        await app.refreshLibrary();
        if (app.gameDir) await app.selectGame(app.gameDir);
      }),
    );
  }

  async function clearMappings() {
    if (!app.profile || !(await app.confirmAsk(t('header.clear_confirm'), 'clear_mappings'))) return;
    app.edit((p) => ({ ...p, mappings: [] }));
  }

  const configItems = $derived<MenuItem[]>([
    ...(app.game?.configs ?? []).map((c) => ({
      label: c.name + (c.error ? ' ⚠' : ''),
      checked: c.path === app.configPath,
      onclick: () => app.openConfig(c.path),
    })),
    { separator: true, label: '' },
    { label: t('header.new_config') + '…', onclick: () => app.newConfig(), disabled: !app.gameDir },
    { label: t('header.import'), onclick: importJson, disabled: !app.gameDir },
    ...TEMPLATES.map((tpl) => ({
      label: t('header.from_template', { template: t(`welcome.templates.${tpl}`) }),
      onclick: () => fromTemplate(tpl),
      disabled: !app.gameDir,
    })),
  ]);

  const optionItems = $derived<MenuItem[]>([
    {
      label: t('header.rename_config') + '…',
      onclick: () => app.renameConfig(),
      disabled: !app.profile || app.readOnly,
      detail: shortcutLabel('rename', mac),
    },
    {
      label: t('common.duplicate'),
      onclick: () => app.duplicateConfig(),
      disabled: !app.configPath,
      detail: shortcutLabel('duplicate', mac),
    },
    { label: t('sheet.menu') + '…', onclick: saveSheet, disabled: !app.profile },
    { label: t('common.reveal'), onclick: () => app.configPath && app.attempt(api.reveal(app.configPath)), disabled: !app.configPath },
    { label: t('header.clear_mappings'), onclick: clearMappings, disabled: !app.profile || app.readOnly },
    { label: t('shortcuts_list.title'), onclick: () => (app.shortcutsOpen = true), detail: shortcutLabel('help', mac) },
    { separator: true, label: '' },
    { label: t('common.delete'), danger: true, onclick: remove, disabled: !app.configPath },
  ]);
</script>

<!-- Doubles as the window's title bar: empty parts drag the window (see main.rs). -->
<header class="header" class:flush={platform === 'windows'} data-tauri-drag-region="deep">
  <div class="crumbs">
    {#if app.game || app.profile}
      <span class="game">{app.game?.meta.name ?? t('profiles.unsorted')}</span>
      <span class="sep">›</span>
      <Menu items={configItems}>
        {#snippet trigger()}
          <span class="config">{configName} <ChevronDown size={14} /></span>
        {/snippet}
      </Menu>
      {#if app.readOnly}<span class="badge" title={t('header.read_only')}><Lock size={12} /> {t('header.read_only')}</span>{/if}
      <button
        class="icon-btn"
        title={withShortcut(t('find.title'), 'find', mac)}
        disabled={!app.profile}
        onclick={() => (app.find = { query: '' })}
      >
        <Search size={17} />
      </button>
      <button
        class="icon-btn"
        title={withShortcut(t('header.undo'), 'undo', mac)}
        disabled={!app.canUndo || app.readOnly}
        onclick={() => app.undo()}
      >
        <Undo2 size={17} />
      </button>
      <button
        class="icon-btn"
        title={withShortcut(t('header.redo'), 'redo', mac)}
        disabled={!app.canRedo || app.readOnly}
        onclick={() => app.redo()}
      >
        <Redo2 size={17} />
      </button>
      <button
        class="icon-btn save"
        class:dirty={app.dirty}
        title={withShortcut(t('header.save'), 'save', mac)}
        disabled={!app.dirty || app.readOnly}
        onclick={() => app.save()}
      >
        <Save size={17} />
      </button>
      <Menu items={optionItems} title={t('header.options')} iconOnly>
        {#snippet trigger()}<span class="icon-btn"><Ellipsis size={17} /></span>{/snippet}
      </Menu>
    {:else}
      <span class="hint">{t('header.no_selection')}</span>
    {/if}
  </div>
  <div class="right">
    <button class="icon-btn" title={t('header.settings')} onclick={onsettings}><Settings size={17} /></button>
    <WindowControls />
  </div>
</header>

<style>
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    border-bottom: 1px solid var(--line);
    min-height: 52px;
  }
  /* Windows caption buttons sit flush with the window's right edge. */
  .header.flush {
    padding-right: 0;
  }
  .right {
    display: flex;
    align-items: center;
    align-self: stretch;
    gap: 8px;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .game {
    color: var(--text-dim);
    font-size: 15px;
  }
  .sep {
    color: var(--text-faint);
  }
  .config {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 15px;
    font-weight: 600;
    padding: 4px 8px;
    border-radius: var(--radius-s);
  }
  .config:hover {
    background: var(--bg-2);
  }
  .save {
    position: relative;
  }
  .save.dirty::after {
    content: '';
    position: absolute;
    top: 5px;
    right: 5px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }
  .crumbs .icon-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-xs);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
    border-radius: var(--radius-pill);
    padding: 2px 8px;
  }
</style>
