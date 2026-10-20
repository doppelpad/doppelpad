<script lang="ts">
  import { Plus, Ellipsis, FileText, Radar } from '@lucide/svelte';
  import { api, errorText } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import { platform, win } from '../lib/state/window.svelte';
  import type { Game } from '../lib/types';
  import Menu from './ui/Menu.svelte';

  /** Stable hue per game name for the avatar. */
  function hue(name: string): number {
    let h = 0;
    for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) % 360;
    return h;
  }

  async function addGame() {
    const name = await app.ask(t('profiles.new_name'));
    if (!name) return;
    try {
      const g = await api.gameCreate(name);
      await app.refreshLibrary();
      await app.selectGame(g.dir);
      app.toast(t('notices.game_created', { name }), 'ok');
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
  }

  async function renameGame(g: Game) {
    const name = await app.ask(t('common.rename'), g.meta.name);
    if (!name) return;
    await api.gameUpdate(g.dir, { ...g.meta, name }).catch((e) => app.toast(errorText(e), 'error'));
    await app.refreshLibrary();
  }

  async function editProcesses(g: Game) {
    const text = await app.ask(t('profiles.processes_hint'), g.meta.processes.join('\n'), true);
    if (text === null) return;
    const processes = text
      .split(/[\n,;]/)
      .map((s) => s.trim())
      .filter(Boolean);
    await api.gameUpdate(g.dir, { ...g.meta, processes }).catch((e) => app.toast(errorText(e), 'error'));
    await app.refreshLibrary();
  }

  async function deleteGame(g: Game) {
    if (!confirm(t('profiles.delete_confirm', { name: g.meta.name }))) return;
    try {
      await api.gameDelete(g.dir);
      if (app.gameDir === g.dir) {
        app.gameDir = null;
        app.closeConfig();
      }
      await app.refreshLibrary();
    } catch (e) {
      app.toast(errorText(e), 'error');
    }
  }

  function gameIsActive(g: Game): boolean {
    const p = app.live?.profile_path;
    return !!p && g.configs.some((c) => c.path === p);
  }
</script>

<aside class="panel">
  <!-- Part of the title bar, beside the header; holds the macOS traffic lights. -->
  <div class="head" class:traffic-lights={platform === 'macos' && !win.fullscreen} data-tauri-drag-region="deep">
    <span class="title">{t('profiles.title')}</span>
    <button class="icon-btn" title={t('profiles.add')} onclick={addGame}><Plus size={18} /></button>
  </div>

  <div class="list">
    {#if app.library.games.length === 0}
      <p class="empty">{t('profiles.empty')}</p>
      <button class="btn primary add" onclick={addGame}><Plus size={16} /> {t('profiles.add')}</button>
    {/if}
    {#each app.library.games as g (g.dir)}
      <div class="game" class:selected={g.dir === app.gameDir}>
        <button class="game-main" onclick={() => app.selectGame(g.dir)}>
          <span class="avatar" style="--h: {hue(g.meta.name)}">{g.meta.name.slice(0, 1).toUpperCase()}</span>
          <span class="name">
            {g.meta.name}
            <span class="sub">
              {g.configs.length === 1 ? t('profiles.config_count_one') : t('profiles.config_count', { n: g.configs.length })}
              {#if g.meta.processes.length}· <Radar size={11} /> {g.meta.processes[0]}{/if}
            </span>
          </span>
          {#if gameIsActive(g)}<span class="live-dot" title={t('devices.in_use')}></span>{/if}
        </button>
        <Menu
          align="right"
          items={[
            { label: t('common.rename'), onclick: () => renameGame(g) },
            { label: t('profiles.autodetect') + '…', onclick: () => editProcesses(g) },
            { label: t('common.reveal'), onclick: () => api.reveal(g.dir) },
            { separator: true, label: '' },
            { label: t('common.delete'), danger: true, onclick: () => deleteGame(g) },
          ]}
        >
          {#snippet trigger()}<span class="icon-btn more"><Ellipsis size={16} /></span>{/snippet}
        </Menu>
      </div>
    {/each}

    {#if app.library.unsorted.length}
      <div class="section">{t('profiles.unsorted')}</div>
      {#each app.library.unsorted as c (c.path)}
        <button
          class="loose"
          class:selected={c.path === app.configPath && !app.gameDir}
          onclick={() => {
            if (!app.confirmDiscard()) return;
            app.gameDir = null;
            app.openConfig(c.path, true);
          }}
        >
          <FileText size={14} />
          <span class="name">{c.name}</span>
          {#if c.error}<span class="err" title={c.error}>!</span>{/if}
        </button>
      {/each}
    {/if}
  </div>
</aside>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    background: var(--bg-1);
    border-right: 1px solid var(--line);
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex: none;
    height: 52px;
    padding: 0 12px 0 16px;
  }
  /* Room for the buttons placed by traffic_light_position in main.rs. */
  .head.traffic-lights {
    padding-left: 84px;
  }
  .title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  .empty {
    color: var(--text-dim);
    padding: 8px;
  }
  .add {
    margin: 0 8px;
  }
  .game {
    display: flex;
    align-items: center;
    border-radius: var(--radius-m);
    margin-bottom: 2px;
    border: 1px solid transparent;
  }
  .game:hover {
    background: var(--bg-2);
  }
  .game.selected {
    background: var(--accent-soft);
    border-color: rgba(20, 184, 166, 0.35);
  }
  .game-main {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    text-align: left;
    min-width: 0;
  }
  .avatar {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 9px;
    display: grid;
    place-items: center;
    font-weight: 600;
    font-size: 15px;
    color: white;
    background: linear-gradient(135deg, hsl(var(--h) 55% 42%), hsl(calc(var(--h) + 40) 55% 30%));
  }
  .name {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 11px;
    color: var(--text-faint);
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    flex: none;
    box-shadow: 0 0 6px var(--ok);
  }
  .more {
    opacity: 0;
  }
  .game:hover .more,
  .game.selected .more {
    opacity: 1;
  }
  .section {
    margin: 14px 8px 6px;
    font-size: 11px;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .loose {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
    text-align: left;
  }
  .loose:hover {
    background: var(--bg-2);
    color: var(--text);
  }
  .loose.selected {
    background: var(--accent-soft);
    color: var(--text);
  }
  .err {
    margin-left: auto;
    color: var(--danger);
    font-weight: 700;
  }
</style>
