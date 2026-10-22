<script lang="ts">
  // Whether the drivers are installed, with a button to look again.
  import { RefreshCw } from '@lucide/svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import DriverList from './DriverList.svelte';
  import SettingsSection from './SettingsSection.svelte';
</script>

<SettingsSection title={t('settings.drivers')}>
  {#snippet actions()}
    <button class="icon-btn" title={t('settings.recheck')} onclick={() => app.attempt(api.refreshDrivers())}><RefreshCw size={15} /></button>
  {/snippet}
  <DriverList tone="icon" />
</SettingsSection>
