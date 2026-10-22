<script lang="ts">
  // The welcome guide on first start: drivers, the controller, what it should become, and a
  // first config made from a template (applied right away if wanted).
  import { CircleCheck } from '@lucide/svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { app } from '../lib/state/app.svelte';
  import type { Template } from '../lib/types';
  import { TEMPLATES } from '../lib/model/labels';
  import IdentifyWizard from './IdentifyWizard.svelte';
  import DriverList, { driverStates } from './settings/DriverList.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Toggle from './ui/Toggle.svelte';

  const STEPS = ['welcome', 'drivers', 'controller', 'output', 'finish'] as const;

  let step = $state(0);
  let template = $state<Template>('virtual_xbox');
  let name = $state(t('welcome.default_game'));
  let useNow = $state(true);
  let busy = $state(false);
  let identify = $state<string | null>(null);

  const live = $derived(app.live);
  const driverMissing = $derived(driverStates(live).some((d) => !d.state.available));
  const devices = $derived(live?.devices ?? []);
  const needsPad = $derived(template !== 'desktop');

  function done() {
    app.setSettings({ welcome_done: true });
  }

  async function create() {
    busy = true;
    const made = api.gameCreateTemplate(name.trim(), t(`welcome.templates.${template}`), template);
    await app.attempt(
      made.then(async (g) => {
        // Opens the new game's config, unless the user keeps unsaved edits of the open one.
        await app.openCreatedGame(g);
        const path = g.configs[0]?.path;
        if (path && useNow && app.gameDir === g.dir) {
          await api.applyToSlot(0, path);
          await api.setRemap(true);
        }
        app.toast(t('welcome.created', { name: g.meta.name }), 'ok');
        done();
      }),
    );
    busy = false;
  }
</script>

<Dialog
  title={t(`welcome.${STEPS[step]}.title`)}
  onclose={done}
  closeLabel={t('welcome.skip')}
  width={500}
  level="guide"
  backdropClose={false}
  escapeClose={false}
>
  {#if STEPS[step] === 'welcome'}
    <p>{t('welcome.welcome.text')}</p>
  {:else if STEPS[step] === 'drivers'}
    <p class="hint">{t('welcome.drivers.text')}</p>
    <DriverList tone="row" />
    {#if driverMissing}<p class="hint">{t('welcome.drivers.after')}</p>{/if}
  {:else if STEPS[step] === 'controller'}
    {#if devices.length === 0}
      <p>{t('welcome.controller.none')}</p>
    {:else}
      <p class="hint">{t('welcome.controller.text')}</p>
      {#each devices as d (d.id)}
        <div class="row">
          <CircleCheck size={15} />
          <span class="grow">{d.name}</span>
          {#if d.family === 'generic' && d.raw}
            <button class="btn" onclick={() => (identify = d.id)}>{t('identify.menu')}</button>
          {/if}
        </div>
      {/each}
    {/if}
  {:else if STEPS[step] === 'output'}
    <p class="hint">{t('welcome.output.text')}</p>
    {#each TEMPLATES as tpl (tpl)}
      <label class="choice">
        <input type="radio" name="template" value={tpl} checked={template === tpl} onchange={() => (template = tpl)} />
        <span>
          <strong>{t(`welcome.templates.${tpl}`)}</strong>
          <span class="hint">{t(`welcome.template_hints.${tpl}`)}</span>
        </span>
      </label>
    {/each}
  {:else}
    <label class="field">
      <span>{t('welcome.finish.name')}</span>
      <input bind:value={name} />
    </label>
    <Toggle checked={useNow} label={t('welcome.finish.use_now')} onchange={(v) => (useNow = v)} />
    {#if needsPad && devices.length === 0 && template !== 'keyboard_mouse'}
      <p class="hint">{t('welcome.finish.no_pad')}</p>
    {/if}
  {/if}

  <div class="actions">
    <button class="btn" onclick={done}>{t('welcome.skip')}</button>
    <span class="grow"></span>
    {#if step > 0}<button class="btn" onclick={() => (step -= 1)}>{t('common.back')}</button>{/if}
    {#if step < STEPS.length - 1}
      <button class="btn primary" onclick={() => (step += 1)}>{t('welcome.next')}</button>
    {:else}
      <button class="btn primary" disabled={busy || !name.trim()} onclick={create}>{t('welcome.finish.create')}</button>
    {/if}
  </div>
</Dialog>

{#if identify}<IdentifyWizard deviceId={identify} onclose={() => (identify = null)} />{/if}

<style>
  p {
    margin: 0;
  }
  .hint {
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .choice {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    cursor: pointer;
  }
  .choice span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 4px;
  }
</style>
