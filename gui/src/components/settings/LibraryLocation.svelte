<script lang="ts">
  // Where the games and configs are: shown with Reveal, moved to another folder (a synced one,
  // say), or back to the default folder. Moving copies and checks every file before the old
  // ones go; a folder that already holds a Doppelpad library is used as it is.
  import { open } from '@tauri-apps/plugin-dialog';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { LibraryPlace } from '../../lib/types';
  import PathRow from './PathRow.svelte';
  import SettingsSection from './SettingsSection.svelte';

  let place = $state<LibraryPlace | null>(null);
  let busy = $state(false);
  const load = () => app.attempt(api.libraryPlace().then((p) => (place = p)));
  load();

  async function choose() {
    const dir = await open({ directory: true, title: t('settings.library.choose') });
    if (typeof dir === 'string') await moveTo(dir);
  }

  /** Move the library to `dir` (`null`: the default folder), after asking. */
  async function moveTo(dir: string | null) {
    if (!place) return;
    const target = dir ?? place.default;
    const old = place.dir;
    const kind = await app.attemptGet(api.libraryTarget(target));
    if (kind === undefined) return;
    if (kind === 'foreign') {
      app.toast(t('settings.library.foreign', { dir: target }), 'error');
      return;
    }
    const question =
      kind === 'empty' ? t('settings.library.move_confirm', { dir: target }) : t('settings.library.use_confirm', { dir: target, old });
    if (!(await app.confirmAsk(question))) return;
    busy = true;
    await app.attempt(
      api.libraryMove(dir).then(async (moved) => {
        await app.libraryMoved(moved.moved_from, moved.dir);
        if (moved.left_behind > 0) app.toast(t('settings.library.left_behind', { n: moved.left_behind, dir: old }), 'error');
        else app.toast(t(moved.moved_from ? 'settings.library.moved' : 'settings.library.used', { dir: moved.dir }), 'ok');
      }),
    );
    busy = false;
    await load();
  }
</script>

{#if place?.portable}
  <p class="hint portable">{t('settings.library.portable', { dir: place.portable })}</p>
{/if}
<SettingsSection title={t('settings.profiles_dir')} hint={t('settings.library.hint')}>
  <PathRow path={place?.dir ?? ''} revealLabel={t('common.reveal')} />
  <div class="buttons">
    <button class="btn" onclick={choose} disabled={busy || !place}>{t('settings.library.change')}</button>
    {#if place && !place.is_default}
      <button class="btn" onclick={() => moveTo(null)} disabled={busy}>{t('common.reset')}</button>
    {/if}
  </div>
</SettingsSection>

<style>
  .buttons {
    display: flex;
    gap: 8px;
  }
</style>
