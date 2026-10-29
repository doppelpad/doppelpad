<script lang="ts">
  import { Download, Ellipsis, FileText, Library, Plus, Radar } from '@lucide/svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import { pictureOf } from '../lib/state/pictures.svelte';
  import { platform, win } from '../lib/state/window.svelte';
  import type { Game } from '../lib/types';
  import Menu from './ui/Menu.svelte';
  import GameDialog from './GameDialog.svelte';
  import LibraryDialog from './LibraryDialog.svelte';
  import { tip } from '../lib/tooltip.svelte';

  /** Stable hue per game name for the avatar. */
  function hue(name: string): number {
    let h = 0;
    for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) % 360;
    return h;
  }

  async function addGame() {
    const name = await app.ask(t('profiles.new_name'));
    if (name) await app.createGame(name);
  }

  async function renameGame(g: Game) {
    const name = await app.ask(t('common.rename'), g.meta.name);
    if (!name) return;
    await app.attempt(api.gameUpdate(g.dir, { ...g.meta, name }));
    await app.refreshLibrary();
  }

  /** The game whose settings dialog is open. */
  let settingsOf = $state<Game | null>(null);
  /** The dialog with the games of the local stores is open. */
  let libraryOpen = $state(false);

  async function duplicateGame(g: Game) {
    const made = api.gameDuplicate(g.dir);
    if (!(await app.attempt(made))) return;
    await app.refreshLibrary();
    await app.selectGame((await made).dir);
  }

  async function exportGame(g: Game) {
    const out = await save({
      title: t('profiles.export_title'),
      defaultPath: `${g.meta.name}.dpgame`,
      filters: [{ name: t('profiles.bundle_filter'), extensions: ['dpgame'] }],
    });
    if (!out) return;
    await app.attempt(api.gameExport(g.dir, out), t('profiles.exported', { name: g.meta.name }));
  }

  async function importGame() {
    const file = await open({
      title: t('profiles.import_title'),
      multiple: false,
      directory: false,
      filters: [{ name: t('profiles.bundle_filter'), extensions: ['dpgame'] }],
    });
    if (typeof file === 'string') await app.importGame(file);
  }

  function gameIsActive(g: Game): boolean {
    const p = app.live?.profile_path;
    return !!p && g.configs.some((c) => c.path === p);
  }
</script>

<aside class="panel">
  <!-- Part of the title bar, beside the header; holds the macOS traffic lights. -->
  <div class="head" class:traffic-lights={platform === 'macos' && !win.fullscreen} data-tauri-drag-region="deep">
    <span class="title section-title">{t('profiles.title')}</span>
    <button class="icon-btn" use:tip={t('stores.menu')} onclick={() => (libraryOpen = true)}><Library size={17} /></button>
    <button class="icon-btn" use:tip={t('profiles.import')} onclick={importGame}><Download size={17} /></button>
    <button class="icon-btn" use:tip={t('profiles.add')} onclick={addGame}><Plus size={18} /></button>
  </div>

  <div class="list">
    {#if app.library.games.length === 0}
      <p class="empty">{t('profiles.empty')}</p>
      <button class="btn primary add" onclick={addGame}><Plus size={16} /> {t('profiles.add')}</button>
    {/if}
    {#each app.library.games as g (g.dir)}
      {@const picture = pictureOf(g.dir, g.meta.image)}
      <div class="game" class:selected={g.dir === app.gameDir}>
        <button class="game-main" onclick={() => app.selectGame(g.dir)}>
          {#if picture}
            <img class="avatar picture" src={picture} alt="" />
          {:else}
            <span class="avatar" style="--h: {hue(g.meta.name)}">{g.meta.name.slice(0, 1).toUpperCase()}</span>
          {/if}
          <span class="name">
            {g.meta.name}
            <span class="sub">
              {g.meta.description ||
                (g.configs.length === 1 ? t('profiles.config_count_one') : t('profiles.config_count', { n: g.configs.length }))}
              {#if g.meta.processes.length}· <Radar size={11} /> {g.meta.processes[0]}{/if}
            </span>
          </span>
          {#if gameIsActive(g)}<span class="live-dot" use:tip={t('devices.in_use')}></span>{/if}
        </button>
        <Menu
          align="right"
          title={t('profiles.game_options')}
          iconOnly
          items={[
            { label: t('common.rename'), onclick: () => renameGame(g) },
            { label: t('game.settings'), onclick: () => (settingsOf = g) },
            { label: t('common.duplicate'), onclick: () => duplicateGame(g) },
            { label: t('common.reveal'), onclick: () => app.attempt(api.reveal(g.dir)) },
            { label: t('profiles.export'), onclick: () => exportGame(g) },
            { separator: true },
            { label: t('common.delete'), danger: true, onclick: () => app.deleteGame(g) },
          ]}
        >
          {#snippet trigger()}<span class="icon-btn more"><Ellipsis size={16} /></span>{/snippet}
        </Menu>
      </div>
    {/each}

    {#if app.library.unsorted.length}
      <div class="section section-title">{t('profiles.unsorted')}</div>
      {#each app.library.unsorted as c (c.path)}
        <button
          class="loose"
          class:selected={c.path === app.configPath && !app.gameDir}
          onclick={async () => {
            if (!(await app.confirmDiscard())) return;
            app.gameDir = null;
            app.openConfig(c.path, true);
          }}
        >
          <FileText size={14} />
          <span class="name">{c.name}</span>
          {#if c.error}<span class="err" use:tip={c.error}>!</span>{/if}
        </button>
      {/each}
    {/if}
  </div>
</aside>

{#if settingsOf}<GameDialog game={settingsOf} onclose={() => (settingsOf = null)} />{/if}
{#if libraryOpen}<LibraryDialog onclose={() => (libraryOpen = false)} />{/if}

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
    gap: 2px;
    flex: none;
    height: 52px;
    padding: 0 12px 0 16px;
  }
  /* Room for the buttons placed by traffic_light_position in main.rs, which end at
     78 px; the wider gap keeps the title apart from them. */
  .head.traffic-lights {
    padding-left: 92px;
  }
  /* Pushes the buttons to the right edge as one group. */
  .title {
    margin-right: auto;
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
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
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
  .avatar.picture {
    object-fit: cover;
    background: var(--bg-2);
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
    font-size: var(--fs-xs);
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
  .game:focus-within .more,
  .game.selected .more {
    opacity: 1;
  }
  .section {
    margin: 14px 8px 6px;
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
