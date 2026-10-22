<script lang="ts">
  // The controllers' player lights and mute LED (settings of the service, so only with it).
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { PlayerLights } from '../../lib/types';
  import Toggle from '../ui/Toggle.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsSection from './SettingsSection.svelte';

  const PLAYER_LIGHTS: PlayerLights[] = ['while_remapping', 'always', 'off'];

  const setLights = (player: PlayerLights, mic: boolean) => app.attempt(api.setLights(player, mic));
</script>

{#if app.live}
  {@const live = app.live}
  <SettingsSection title={t('settings.lights.title')}>
    <SettingRow label={t('settings.lights.player')}>
      <select value={live.player_lights} onchange={(e) => setLights(e.currentTarget.value as PlayerLights, live.mic_led)}>
        {#each PLAYER_LIGHTS as p (p)}<option value={p}>{t(`settings.lights.players.${p}`)}</option>{/each}
      </select>
    </SettingRow>
    <Toggle checked={live.mic_led} label={t('settings.lights.mic')} onchange={(v) => setLights(live.player_lights, v)} />
  </SettingsSection>
{/if}

<style>
  select {
    min-width: 160px;
  }
</style>
