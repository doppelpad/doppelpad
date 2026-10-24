<script lang="ts">
  // The screen corner an overlay sits in, as a settings row.
  import { t } from '../../lib/i18n/index.svelte';
  import type { Corner } from '../../lib/types';
  import Select from '../ui/Select.svelte';
  import SettingRow from './SettingRow.svelte';

  let { value, onchange }: { value: Corner; onchange: (corner: Corner) => void } = $props();

  const CORNERS: Corner[] = ['top_left', 'top_right', 'bottom_left', 'bottom_right'];
</script>

<SettingRow label={t('settings.osd.corner')}>
  <Select {value} class="setting" options={CORNERS.map((c) => ({ value: c, label: t(`settings.osd.corners.${c}`) }))} {onchange} />
</SettingRow>
