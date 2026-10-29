<script lang="ts">
  // A game's settings: name, description, picture, and the programs that switch to it
  // automatically. Programs come from the running programs, a file picker, the program in front
  // a few seconds after asking, or typed in. Saved with the dialog's button.
  import { AppWindow, FolderOpen, ImagePlus, Timer, Trash2, X } from '@lucide/svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { onDestroy } from 'svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { addProgram, hasProgram } from '../lib/model/games';
  import { app } from '../lib/state/app.svelte';
  import { forgetPicture, pictureOf } from '../lib/state/pictures.svelte';
  import type { Game } from '../lib/types';
  import Dialog from './ui/Dialog.svelte';
  import SettingRow from './settings/SettingRow.svelte';
  import { tip } from '../lib/tooltip.svelte';

  let { game, onclose }: { game: Game; onclose: () => void } = $props();

  // A copy to edit; nothing changes until Save.
  // svelte-ignore state_referenced_locally
  let name = $state(game.meta.name);
  // svelte-ignore state_referenced_locally
  let description = $state(game.meta.description ?? '');
  // svelte-ignore state_referenced_locally
  let programs = $state<string[]>([...game.meta.processes]);
  let typed = $state('');
  /** The picture file; it changes at once with its own buttons. */
  // svelte-ignore state_referenced_locally
  let image = $state(game.meta.image);
  const picture = $derived(pictureOf(game.dir, image));
  let running = $state<{ exe: string; title: string }[] | null>(null);
  let filter = $state('');
  /** Seconds left before the program in front is taken, while counting down. */
  let countdown = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  const shown = $derived(
    (running ?? []).filter((p) => {
      const q = filter.trim().toLowerCase();
      return !q || p.exe.toLowerCase().includes(q) || p.title.toLowerCase().includes(q);
    }),
  );

  function add(exe: string) {
    programs = addProgram(programs, exe);
  }

  async function showRunning() {
    if (running) running = null;
    else running = (await app.attemptGet(api.runningPrograms())) ?? null;
  }

  async function browse() {
    const file = await open({
      title: t('game.browse_title'),
      multiple: false,
      directory: false,
      filters: [{ name: t('game.programs_filter'), extensions: ['exe'] }],
    });
    if (typeof file === 'string') add(file.split(/[\\/]/).pop() ?? '');
  }

  /** Switch to the game; after three seconds the program in front is taken. */
  function inFront() {
    clearInterval(timer);
    countdown = 3;
    timer = setInterval(async () => {
      countdown -= 1;
      if (countdown > 0) return;
      clearInterval(timer);
      const exe = await app.attemptGet(api.foregroundProgram());
      if (exe === undefined) return;
      if (exe && !/^doppelpad/i.test(exe)) add(exe);
      else app.toast(t('game.in_front_none'), 'info');
    }, 1000);
  }
  onDestroy(() => clearInterval(timer));

  async function choosePicture() {
    const file = await open({
      title: t('game.picture_title'),
      multiple: false,
      directory: false,
      filters: [{ name: t('game.picture_filter'), extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
    });
    if (typeof file === 'string') await setPicture(file);
  }

  /** Copy a picture into the game's folder, or (`null`) remove it; the games list shows it too. */
  async function setPicture(source: string | null) {
    const meta = await app.attemptGet(api.gameSetPicture(game.dir, source));
    if (!meta) return;
    forgetPicture(game.dir);
    image = meta.image;
    await app.attempt(app.refreshLibrary());
  }

  async function save() {
    add(typed);
    const meta = { ...game.meta, name: name.trim() || game.meta.name, processes: programs, description: description.trim() || undefined };
    // The picture is saved at once by its own buttons; keep what is on disk.
    const fresh = app.library.games.find((g) => g.dir === game.dir)?.meta;
    if (fresh) meta.image = fresh.image;
    if (await app.attempt(api.gameUpdate(game.dir, meta))) {
      await app.refreshLibrary();
      onclose();
    }
  }
</script>

<Dialog title={t('game.title', { name: game.meta.name })} {onclose} width={520} gap={14}>
  <SettingRow label={t('game.name')}>
    <input bind:value={name} />
  </SettingRow>
  <label class="block">
    <span class="label">{t('game.description')}</span>
    <textarea rows="2" bind:value={description} placeholder={t('game.description_hint')}></textarea>
  </label>

  <div class="picture">
    {#if picture}<img src={picture} alt="" />{:else}<span class="empty"><ImagePlus size={22} /></span>{/if}
    <div class="picture-buttons">
      <button class="btn" onclick={choosePicture}><ImagePlus size={14} /> {t('game.picture_choose')}</button>
      {#if picture}<button class="btn" onclick={() => setPicture(null)}><Trash2 size={14} /> {t('common.remove')}</button>{/if}
    </div>
  </div>

  <div class="block">
    <span class="label">{t('game.programs')}</span>
    <p class="hint">{t('game.programs_hint')}</p>
    {#each programs as p (p)}
      <div class="program">
        <span>{p}</span>
        <button class="icon-btn" use:tip={t('common.remove')} onclick={() => (programs = programs.filter((x) => x !== p))}
          ><X size={14} /></button
        >
      </div>
    {/each}
    <div class="add-row">
      <input
        bind:value={typed}
        placeholder={t('game.program_placeholder')}
        aria-label={t('game.program_placeholder')}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            add(typed);
            typed = '';
          }
        }}
      />
      <button
        class="btn"
        onclick={() => {
          add(typed);
          typed = '';
        }}>{t('game.add')}</button
      >
    </div>
    <div class="sources">
      <button class="btn" class:on={running !== null} aria-expanded={running !== null} onclick={showRunning}
        ><AppWindow size={14} /> {t('game.running')}</button
      >
      <button class="btn" onclick={browse}><FolderOpen size={14} /> {t('game.browse')}</button>
      <button class="btn" onclick={inFront} disabled={countdown > 0}>
        <Timer size={14} />
        {countdown > 0 ? t('game.in_front_wait', { n: countdown }) : t('game.in_front')}
      </button>
    </div>
    {#if running}
      <div class="running">
        <input bind:value={filter} placeholder={t('game.filter')} aria-label={t('game.filter')} />
        {#each shown as r (r.exe)}
          <button class="pick" onclick={() => add(r.exe)} disabled={hasProgram(programs, r.exe)}>
            <span class="exe">{r.exe}</span>
            <span class="title">{r.title}</span>
          </button>
        {:else}
          <p class="hint">{t('game.running_none')}</p>
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t('common.cancel')}</button>
    <button class="btn primary" onclick={save}>{t('game.save')}</button>
  {/snippet}
</Dialog>

<style>
  .block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .label {
    font-weight: 500;
  }
  textarea {
    resize: vertical;
  }
  .picture {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .picture img,
  .picture .empty {
    width: 96px;
    height: 54px;
    object-fit: cover;
    border-radius: var(--radius-s);
    border: 1px solid var(--line);
  }
  .picture .empty {
    display: grid;
    place-items: center;
    color: var(--text-faint);
  }
  .picture-buttons,
  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .program {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
  }
  .add-row {
    display: flex;
    gap: 6px;
  }
  .add-row input {
    flex: 1;
  }
  .btn.on {
    border-color: var(--accent);
  }
  .running {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 200px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
    padding: 6px;
  }
  .pick {
    display: flex;
    gap: 8px;
    text-align: left;
    padding: 4px 6px;
    border-radius: var(--radius-s);
  }
  .pick:hover:not(:disabled) {
    background: var(--bg-3);
  }
  .pick:disabled {
    opacity: 0.4;
  }
  .exe {
    font-weight: 500;
    white-space: nowrap;
  }
  .title {
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
