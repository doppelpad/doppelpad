<script lang="ts">
  // The config's output page: the virtual controller and how input passes, keyboard and mouse,
  // turning, shaking, timing, light bar, vibration, crosshairs and radial menus.
  import { t } from '../lib/i18n/index.svelte';
  import CrosshairSettings from './crosshair/CrosshairSettings.svelte';
  import InputSettings from './output/InputSettings.svelte';
  import LightSettings from './output/LightSettings.svelte';
  import MouseSettings from './output/MouseSettings.svelte';
  import ShakeSettings from './output/ShakeSettings.svelte';
  import TimingSettings from './output/TimingSettings.svelte';
  import VibrationSettings from './output/VibrationSettings.svelte';
  import VirtualPadSettings from './output/VirtualPadSettings.svelte';
  import RadialSettings from './radial/RadialSettings.svelte';
</script>

<div class="output">
  <h2>{t('views.output')}</h2>
  <VirtualPadSettings />
  <InputSettings />
  <MouseSettings />
  <ShakeSettings />
  <TimingSettings />
  <LightSettings />
  <VibrationSettings />
  <CrosshairSettings />
  <RadialSettings />
</div>

<style>
  .output {
    max-width: 640px;
    margin: 0 auto;
    padding: 32px 28px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
    font-weight: 600;
  }
</style>
