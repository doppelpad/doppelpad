<script lang="ts">
  // The screen corner an overlay sits in, as a settings row.
  import { t } from '../../lib/i18n/index.svelte';
  import type { Corner } from '../../lib/types';
  import SettingRow from './SettingRow.svelte';

  let { value, onchange }: { value: Corner; onchange: (corner: Corner) => void } = $props();

  const CORNERS: Corner[] = ['top_left', 'top_right', 'bottom_left', 'bottom_right'];
</script>

<SettingRow label={t('settings.osd.corner')}>
  <select {value} onchange={(e) => onchange(e.currentTarget.value as Corner)}>
    {#each CORNERS as c (c)}<option value={c}>{t(`settings.osd.corners.${c}`)}</option>{/each}
  </select>
</SettingRow>

<style>
  select {
    min-width: 160px;
  }
</style>
