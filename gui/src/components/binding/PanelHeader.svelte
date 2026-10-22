<script lang="ts">
  // The head of a side panel: what is selected, the layer it is edited on, and a close button.
  import { X } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import LayerBadge from '../ui/LayerBadge.svelte';

  let { title, layer, onclose }: { title: string; layer: number; onclose: () => void } = $props();
</script>

<div class="head">
  <div class="title">
    <span class="name">{title}</span>
    <LayerBadge {layer} />
  </div>
  <button class="icon-btn" title={t('common.close')} onclick={onclose}><X size={16} /></button>
</div>

<style>
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }
  .title {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    min-width: 0;
  }
  .name {
    font-size: 16px;
    font-weight: 600;
  }
</style>
