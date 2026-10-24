<script lang="ts">
  // The layer a mapping jumps to (or a new one), how (while held, toggle, switch), the switch
  // delay and whether the actions wait for it.
  import { t } from '../../lib/i18n/index.svelte';
  import { addLayer, layerIds } from '../../lib/model/profile';
  import { Plus } from '@lucide/svelte';
  import { app } from '../../lib/state/app.svelte';
  import { layerColor } from '../../lib/theme/layers';
  import type { JumpMode, Mapping } from '../../lib/types';
  import Segmented from '../ui/Segmented.svelte';
  import Select from '../ui/Select.svelte';
  import { ms, type EditMapping } from './edit';
  import MsInput from './MsInput.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  let { mapping, readOnly, edit }: { mapping: Mapping | undefined; readOnly: boolean; edit: EditMapping } = $props();

  const otherLayers = $derived(app.profile ? layerIds(app.profile).filter((id) => id !== app.layer) : []);

  function setJumpLayer(value: string) {
    if (value === '') {
      edit((m) => (m.jump = undefined));
      return;
    }
    if (value === 'new' && app.profile) {
      const { profile: p, id } = addLayer(app.profile);
      app.update(p);
      edit((m) => (m.jump = { layer: id }));
      return;
    }
    const layer = Number(value);
    edit((m) => (m.jump = { ...(m.jump ?? {}), layer }));
  }

  function setJumpMode(mode: JumpMode) {
    edit((m) => m.jump && (m.jump = { ...m.jump, mode }));
  }
</script>

<div class="jump">
  <Select
    value={mapping?.jump ? String(mapping.jump.layer) : ''}
    disabled={readOnly}
    wide
    options={[
      { value: '', label: t('binding.jump_none') },
      ...otherLayers.map((id) => ({
        value: String(id),
        label: t('binding.jump_to', { layer: app.layerLabel(id) }),
        color: layerColor(id),
      })),
      { separator: true },
      { value: 'new', label: t('layers.add'), icon: Plus },
    ]}
    onchange={setJumpLayer}
  />
  {#if mapping?.jump}
    <Segmented
      variant="fill"
      label={t('binding.jump')}
      options={(['hold', 'toggle', 'switch'] as const).map((jm) => ({ value: jm, label: t(`binding.jump_${jm}`) }))}
      value={mapping.jump.mode ?? 'hold'}
      disabled={readOnly}
      onchange={setJumpMode}
    />
    {#if mapping.jump.mode === 'switch' || mapping.jump.delay_ms}
      <label class="inline">
        {t('binding.jump_delay')}
        <MsInput
          value={mapping.jump.delay_ms}
          placeholder="0"
          step={50}
          disabled={readOnly}
          onchange={(v) => edit((m) => m.jump && (m.jump = { ...m.jump, delay_ms: ms(v) }))}
        />
      </label>
      {#if mapping.jump.delay_ms}
        <label class="inline" use:tip={t('binding.jump_postpone_hint')}>
          <input
            type="checkbox"
            checked={mapping.jump.postpone ?? false}
            disabled={readOnly}
            onchange={(e) => edit((m) => m.jump && (m.jump = { ...m.jump, postpone: e.currentTarget.checked || undefined }))}
          />
          {t('binding.jump_postpone')}
        </label>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .jump {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
