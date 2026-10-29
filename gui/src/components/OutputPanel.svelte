<script lang="ts">
  // The config's settings, a page at a time: the virtual controller and how input passes,
  // keyboard and mouse, timing, light bar and rumble, crosshairs. The list on the left picks
  // the page, as in the app's settings.
  import { Crosshair, Gamepad2, Keyboard, Lightbulb, Timer } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { OUTPUT_PAGES as PAGE_IDS, type OutputPage } from '../lib/model/views';
  import { app } from '../lib/state/app.svelte';
  import PageTabs from './ui/PageTabs.svelte';
  import CrosshairSettings from './crosshair/CrosshairSettings.svelte';
  import InputSettings from './output/InputSettings.svelte';
  import LightSettings from './output/LightSettings.svelte';
  import MouseSettings from './output/MouseSettings.svelte';
  import ShakeSettings from './output/ShakeSettings.svelte';
  import TimingSettings from './output/TimingSettings.svelte';
  import VibrationSettings from './output/VibrationSettings.svelte';
  import VirtualPadSettings from './output/VirtualPadSettings.svelte';

  const ICONS: Record<OutputPage, typeof Gamepad2> = {
    controller: Gamepad2,
    input: Keyboard,
    timing: Timer,
    feedback: Lightbulb,
    crosshairs: Crosshair,
  };
  const pages = $derived(PAGE_IDS.map((id) => ({ id, icon: ICONS[id], label: t(`output.pages.${id}`) })));
  const page = $derived(app.outputPage);
</script>

<div class="output">
  <h2>{t('views.output')}</h2>
  <PageTabs {pages} current={page} onchange={(p) => (app.outputPage = p)} label={t('views.output')}>
    {#if page === 'controller'}
      <VirtualPadSettings />
      <ShakeSettings />
    {:else if page === 'input'}
      <InputSettings />
      <MouseSettings />
    {:else if page === 'timing'}
      <TimingSettings />
    {:else if page === 'feedback'}
      <LightSettings />
      <VibrationSettings />
    {:else}
      <CrosshairSettings />
    {/if}
  </PageTabs>
</div>

<style>
  .output {
    max-width: 860px;
    margin: 0 auto;
    padding: 28px 28px 40px;
  }
  h2 {
    margin: 0 0 18px;
    font-size: 18px;
    font-weight: 600;
  }
</style>
