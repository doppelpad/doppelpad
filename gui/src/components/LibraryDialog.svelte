<script lang="ts">
  // New games from the local game stores: the games Steam, Epic and GOG have installed, read
  // from their own files. Adding one creates a game with its name, the chosen program (for
  // automatic switching) and, from Steam, its cover. Games already in the list are marked.
  import { Plus, Check } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Select from './ui/Select.svelte';
  import { tip } from '../lib/tooltip.svelte';

  let { onclose }: { onclose: () => void } = $props();

  type Installed = Awaited<ReturnType<typeof api.installedGames>>[number];

  let games = $state<Installed[] | null>(null);
  let filter = $state('');
  /** The program chosen per game folder (the likeliest first). */
  let chosen = $state<Record<string, string>>({});

  app.attempt(api.installedGames().then((g) => (games = g)));

  const shown = $derived((games ?? []).filter((g) => !filter.trim() || g.name.toLowerCase().includes(filter.trim().toLowerCase())));

  /** Already a game: by name, or one of its programs switches to a game. */
  function known(g: Installed): boolean {
    const name = g.name.toLowerCase();
    return app.library.games.some(
      (x) =>
        x.meta.name.toLowerCase() === name || x.meta.processes.some((p) => g.programs.some((q) => q.toLowerCase() === p.toLowerCase())),
    );
  }

  async function add(g: Installed) {
    const program = chosen[g.dir] ?? g.programs[0] ?? null;
    const made = api.gameCreateFromStore(g.name, program, g.cover);
    if (!(await app.attempt(made, t('stores.added', { name: g.name })))) return;
    await app.refreshLibrary();
  }
</script>

<Dialog title={t('stores.title')} {onclose} width={600} gap={12}>
  <p class="hint">{t('stores.hint')}</p>
  <input bind:value={filter} placeholder={t('game.filter')} aria-label={t('game.filter')} />
  <div class="list">
    {#if games === null}
      <p class="hint">{t('stores.loading')}</p>
    {:else}
      {#each shown as g (g.dir)}
        {@const done = known(g)}
        <div class="row">
          <span class="store">{t(`stores.${g.store}`)}</span>
          <span class="name" use:tip={g.dir}>{g.name}</span>
          {#if g.programs.length > 1}
            <Select
              value={chosen[g.dir] ?? g.programs[0]}
              title={t('stores.program')}
              disabled={done}
              size="s"
              options={g.programs.map((p) => ({ value: p, label: p }))}
              onchange={(p) => (chosen[g.dir] = p)}
            />
          {:else}
            <span class="program">{g.programs[0] ?? '—'}</span>
          {/if}
          {#if done}
            <span class="done"><Check size={14} /> {t('stores.in_list')}</span>
          {:else}
            <button class="btn" onclick={() => add(g)}><Plus size={14} /> {t('stores.add')}</button>
          {/if}
        </div>
      {:else}
        <p class="hint">{t('stores.none')}</p>
      {/each}
    {/if}
  </div>
</Dialog>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: min(460px, 60vh);
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: 52px 1fr 180px 110px;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: var(--radius-s);
  }
  .store {
    font-size: var(--fs-xs);
    color: var(--text-dim);
  }
  .name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .program {
    color: var(--text-dim);
    font-size: var(--fs-s);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .done {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
