<script lang="ts">
  // A wireless controller's charge: an icon for the level, the percentage, warning colours.
  import { BatteryCharging, BatteryFull, BatteryLow, BatteryMedium, BatteryWarning } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { BATTERY_CRITICAL, BATTERY_LOW, batteryTone } from '../../lib/model/devices';
  import type { Battery } from '../../lib/types';

  let { battery: b }: { battery: Battery } = $props();

  const tone = $derived(batteryTone(b));
</script>

<span
  class="battery"
  class:low={tone === 'warning'}
  class:critical={tone === 'danger'}
  title={t(b.charging ? 'devices.battery_charging' : 'devices.battery', { level: b.level })}
>
  {#if b.charging}<BatteryCharging size={14} />{:else if b.level < BATTERY_CRITICAL}<BatteryWarning
      size={14}
    />{:else if b.level < BATTERY_LOW}<BatteryLow size={14} />{:else if b.level < 80}<BatteryMedium size={14} />{:else}<BatteryFull
      size={14}
    />{/if}
  {b.level} %
</span>

<style>
  .battery {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--text-dim);
    font-size: var(--fs-xs);
  }
  .battery.low {
    color: var(--warning);
  }
  .battery.critical {
    color: var(--danger);
  }
</style>
