<script lang="ts">
  // Controller hotkeys (app settings): record a combo by holding its buttons on the
  // controller and letting go, clear it, or change the hold time.
  import { CircleDot, X } from '@lucide/svelte';
  import { onDestroy } from 'svelte';
  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { controlLabel } from '../lib/model/controls';
  import { padButtonsDown } from '../lib/model/devices';
  import { app } from '../lib/state/app.svelte';
  import type { HotkeyField, Hotkeys, PadButtonName } from '../lib/types';

  const FIELDS: HotkeyField[] = ['slot1', 'slot2', 'slot3', 'slot4', 'remap', 'stop_macros', 'emergency_stop', 'pad_overlay', 'mappings_overlay'];
  const MAX_BUTTONS = 4;

  let hotkeys = $state<Hotkeys | null>(null);
  app.attempt(api.hotkeysGet().then((h) => (hotkeys = h)));

  let recording = $state<HotkeyField | null>(null);
  /** Buttons pressed since the recording started, in catalog order once saved. */
  let seen = $state<PadButtonName[]>([]);

  /** Pad buttons held on the controller in use. */
  const down = $derived(app.primary ? padButtonsDown(app.primary, app.catalog?.pad_buttons ?? []) : []);

  $effect(() => {
    if (!recording) return;
    const now = down;
    if (now.length > 0) {
      const add = now.filter((b) => !seen.includes(b));
      if (add.length) seen = [...seen, ...add];
    } else if (seen.length > 0) {
      finish();
    }
  });

  const label = (buttons: PadButtonName[]) =>
    buttons.length ? buttons.map((b) => controlLabel(b, app.family)).join(' + ') : t('hotkeys.none');

  async function save(next: Hotkeys) {
    if (await app.attempt(api.hotkeysSet(next))) hotkeys = next;
  }

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
    if (!field || !hotkeys) return;
    if (buttons.length > MAX_BUTTONS) {
      app.toast(t('hotkeys.too_many', { n: MAX_BUTTONS }), 'error');
      return;
    }
    await save({ ...hotkeys, [field]: buttons });
  }

  function setHold(raw: string) {
    const n = parseInt(raw, 10);
    if (!hotkeys || !Number.isFinite(n)) return;
    save({ ...hotkeys, hold_ms: Math.min(Math.max(n, 50), 2500) });
  }

  onDestroy(() => {
    if (recording) stop();
  });
</script>

<div class="section">
  <div class="section-head"><span>{t('hotkeys.title')}</span></div>
  <p class="hint">{t('hotkeys.hint')}</p>
  {#if hotkeys}
    <label class="row">
      <span>{t('hotkeys.hold_ms')}</span>
      <span class="hold">
        <input
          type="number"
          min="50"
          max="2500"
          step="50"
          value={hotkeys.hold_ms}
          onchange={(e) => setHold(e.currentTarget.value)}
        />
        {t('common.ms')}
      </span>
    </label>
    {#each FIELDS as f (f)}
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
          <button class="icon-btn" title={t('hotkeys.record')} disabled={!!recording} onclick={() => start(f)}>
            <CircleDot size={15} />
          </button>
          <button
            class="icon-btn"
            title={t('common.clear')}
            disabled={!!recording || !hotkeys[f].length}
            onclick={() => hotkeys && save({ ...hotkeys, [f]: [] })}
          >
            <X size={15} />
          </button>
        {/if}
      </div>
    {/each}
  {/if}
</div>

<style>
  .section {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .section-head {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-dim);
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
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
