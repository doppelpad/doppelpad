<script lang="ts">
  // The controller and mappings overlays (shown by hotkey): corner, size, opacity and when the
  // controller overlay hides again.
  import { t } from '../../lib/i18n/index.svelte';
  import { app } from '../../lib/state/app.svelte';
  import type { MappingsOverlaySettings, PadOverlaySettings } from '../../lib/types';
  import Slider from '../ui/Slider.svelte';
  import CornerSelect from './CornerSelect.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsSection from './SettingsSection.svelte';

  const pad = $derived(app.settings.pad_overlay);
  const mappings = $derived(app.settings.mappings_overlay);
  const PAD_SIZES = [50, 70, 90] as const;

  function setPad(patch: Partial<PadOverlaySettings>) {
    app.setSettings((s) => ({ pad_overlay: { ...s.pad_overlay, ...patch } }));
  }

  function setMappings(patch: Partial<MappingsOverlaySettings>) {
    app.setSettings((s) => ({ mappings_overlay: { ...s.mappings_overlay, ...patch } }));
  }
</script>

<SettingsSection title={t('settings.overlays.title')} hint={t('settings.overlays.hint')}>
  <div class="sub">{t('settings.overlays.pad')}</div>
  <CornerSelect value={pad.corner} onchange={(corner) => setPad({ corner })} />
  <SettingRow label={t('settings.osd.scale')}>
    <select value={pad.size} onchange={(e) => setPad({ size: Number(e.currentTarget.value) })}>
      {#each PAD_SIZES as s (s)}<option value={s}>{t(`settings.overlays.sizes.${s}`)}</option>{/each}
    </select>
  </SettingRow>
  <Slider
    label={t('settings.osd.opacity')}
    value={pad.opacity}
    def={90}
    min={20}
    max={100}
    step={5}
    unit="%"
    onchange={(v) => setPad({ opacity: v })}
  />
  <Slider
    label={t('settings.overlays.hide_after')}
    value={pad.hide_after}
    def={5}
    min={0}
    max={30}
    step={1}
    unit="s"
    hint={t('settings.overlays.hide_after_hint')}
    onchange={(v) => setPad({ hide_after: v })}
  />
  <div class="sub">{t('settings.overlays.mappings')}</div>
  <CornerSelect value={mappings.corner} onchange={(corner) => setMappings({ corner })} />
  <Slider
    label={t('settings.osd.opacity')}
    value={mappings.opacity}
    def={90}
    min={20}
    max={100}
    step={5}
    unit="%"
    onchange={(v) => setMappings({ opacity: v })}
  />
</SettingsSection>

<style>
  select {
    min-width: 160px;
  }
  .sub {
    margin-top: 4px;
    color: var(--text-dim);
    font-weight: 600;
  }
</style>
