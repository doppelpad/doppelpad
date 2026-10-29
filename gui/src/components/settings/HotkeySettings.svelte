<script lang="ts">
  // Controller hotkeys (app settings): record a combo by holding its buttons on the
  // controller and letting go, clear it, or change the hold time.
  import { CircleDot, X } from '@lucide/svelte';
  import { onDestroy, untrack } from 'svelte';
  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { padButtonsDown } from '../../lib/model/devices';
  import { HOTKEY_FIELDS, hotkeyLabel } from '../../lib/model/hotkeys';
  import { clamp } from '../../lib/model/util';
  import { app } from '../../lib/state/app.svelte';
  import type { HotkeyField, PadButtonName } from '../../lib/types';
  import SettingRow from './SettingRow.svelte';
  import SettingsSection from './SettingsSection.svelte';
  import Toggle from '../ui/Toggle.svelte';
  import { tip } from '../../lib/tooltip.svelte';

  const MAX_BUTTONS = 4;
  /** How long a hotkey must be held, in ms (dp-core's HOLD_MS_RANGE). */
  const HOLD_MS = { min: 50, max: 2500, step: 50 };

  const hotkeys = $derived(app.hotkeys);
  // The service may have changed them since start-up (or was not running then).
  app.refreshHotkeys();

  let recording = $state<HotkeyField | null>(null);
  /** Buttons pressed since the recording started, in catalog order once saved. */
  let seen = $state<PadButtonName[]>([]);

  /** Pad buttons held on the controller in use. */
  const down = $derived(app.primary ? padButtonsDown(app.primary.state, app.catalog?.pad_buttons ?? []) : []);

  $effect(() => {
    const now = down;
    if (!recording) return;
    untrack(() => {
      if (now.length > 0) {
        const add = now.filter((b) => !seen.includes(b));
        if (add.length) seen = [...seen, ...add];
      } else if (seen.length > 0) {
        finish();
      }
    });
  });

  const label = (buttons: PadButtonName[]) => (buttons.length ? hotkeyLabel(buttons, app.family) : t('hotkeys.none'));

  /** Gives the presses back to the editor. */
  let releasePresses = () => {};

  async function start(field: HotkeyField) {
    await api.hotkeysPause(true).catch(() => {});
    seen = [];
    recording = field;
    // Presses must not select controls in the editor meanwhile.
    releasePresses = app.capturePresses();
  }

  async function stop() {
    recording = null;
    seen = [];
    releasePresses();
    await api.hotkeysPause(false).catch(() => {});
  }

  async function finish() {
    const field = recording;
    const order = app.catalog?.pad_buttons ?? [];
    const buttons = [...seen].sort((a, b) => order.indexOf(a) - order.indexOf(b));
    await stop();
    if (!field) return;
    if (buttons.length > MAX_BUTTONS) {
      app.toast(t('hotkeys.too_many', { n: MAX_BUTTONS }), 'error');
      return;
    }
    await app.setHotkeys({ [field]: buttons });
  }

  function setHold(raw: string) {
    const n = parseInt(raw, 10);
    if (Number.isFinite(n)) app.setHotkeys({ hold_ms: clamp(n, HOLD_MS.min, HOLD_MS.max) });
  }

  onDestroy(() => {
    if (recording) stop();
  });
</script>

<SettingsSection title={t('hotkeys.title')} hint={t('hotkeys.hint')}>
  {#if hotkeys}
    <SettingRow label={t('hotkeys.hold_ms')}>
      <span class="hold">
        <input
          type="number"
          min={HOLD_MS.min}
          max={HOLD_MS.max}
          step={HOLD_MS.step}
          value={hotkeys.hold_ms}
          onchange={(e) => setHold(e.currentTarget.value)}
        />
        {t('common.ms')}
      </span>
    </SettingRow>
    <Toggle checked={hotkeys.rumble} label={t('hotkeys.rumble')} onchange={(v) => app.setHotkeys({ rumble: v })} />
    <p class="hint">{t('hotkeys.rumble_hint')}</p>
    {#each HOTKEY_FIELDS as f (f)}
      {@const rec = recording === f}
      <div class="hk">
        <span class="what">{t(`hotkeys.${f}`)}</span>
        <span class="combo" class:off={!hotkeys[f].length} class:rec>
          {#if rec}
            {seen.length ? label(seen) : t('hotkeys.press')}
          {:else}
            {label(hotkeys[f])}
          {/if}
        </span>
        {#if rec}
          <button class="btn" onclick={stop}>{t('common.cancel')}</button>
        {:else}
          <button class="icon-btn" use:tip={t('hotkeys.record')} disabled={!!recording} onclick={() => start(f)}>
            <CircleDot size={15} />
          </button>
          <button
            class="icon-btn"
            use:tip={t('common.clear')}
            disabled={!!recording || !hotkeys[f].length}
            onclick={() => app.setHotkeys({ [f]: [] })}
          >
            <X size={15} />
          </button>
        {/if}
      </div>
    {/each}
  {/if}
</SettingsSection>

<style>
  .hold {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
  }
  .hold input {
    width: 80px;
  }
  .hk {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .what {
    flex: 0 0 150px;
  }
  .combo {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
    border-radius: var(--radius-s);
    background: var(--bg-2);
    border: 1px solid var(--line);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .combo.off {
    color: var(--text-faint);
    font-style: italic;
  }
  .combo.rec {
    border-color: var(--accent);
    color: var(--accent);
    font-style: normal;
  }
</style>
