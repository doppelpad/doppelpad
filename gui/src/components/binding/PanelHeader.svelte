<script lang="ts">
  // The head of a side panel: what is selected, the layer it is edited on, an optional menu
  // (copy, paste, clear) and a close button.
  import { Ellipsis, X } from '@lucide/svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import LayerBadge from '../ui/LayerBadge.svelte';
  import Menu, { type MenuItem } from '../ui/Menu.svelte';

  let { title, layer, menu, onclose }: { title: string; layer: number; menu?: MenuItem[]; onclose: () => void } = $props();
</script>

<div class="head">
  <div class="title">
    <span class="name">{title}</span>
    <LayerBadge {layer} />
  </div>
  <div class="actions">
    {#if menu?.length}
      <Menu items={menu} align="right" title={t('header.options')} iconOnly>
        {#snippet trigger()}<span class="icon-btn"><Ellipsis size={16} /></span>{/snippet}
      </Menu>
    {/if}
    <button class="icon-btn" title={t('common.close')} onclick={onclose}><X size={16} /></button>
  </div>
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
  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
</style>
