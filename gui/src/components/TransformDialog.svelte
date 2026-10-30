<script lang="ts">
  // One-click layout transforms for the current layer or the whole config (lib/model/transforms.ts).
  // They are undoable, so the dialog applies at once and closes.
  import { ArrowLeftRight, Gamepad2, FlipHorizontal2, Joystick } from '@lucide/svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { TRANSFORMS, transform, type TransformKind } from '../lib/model/transforms';
  import { app } from '../lib/state/app.svelte';
  import Dialog from './ui/Dialog.svelte';
  import Segmented from './ui/Segmented.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let scope = $state<'layer' | 'all'>('layer');

  const ICONS = { face: Gamepad2, sticks: Joystick, bumpers_triggers: ArrowLeftRight, mirror: FlipHorizontal2 } as const;

  function run(kind: TransformKind) {
    const def = app.catalog?.defaults.stick;
    if (!def) return;
    const target = scope === 'all' ? 'all' : app.layer;
    app.edit((p) => transform(p, kind, target, def));
    app.toast(t('transforms.done', { name: t(`transforms.${kind}`) }), 'ok');
    onclose();
  }
</script>

<Dialog title={t('transforms.title')} {onclose} width={460}>
  <Segmented
    variant="fill"
    label={t('transforms.scope')}
    options={[
      { value: 'layer', label: t('transforms.this_layer', { layer: app.layerLabel(app.layer) }) },
      { value: 'all', label: t('transforms.whole_config') },
    ]}
    value={scope}
    onchange={(v) => (scope = v)}
  />
  <div class="choices">
    {#each TRANSFORMS as kind (kind)}
      {@const Icon = ICONS[kind]}
      <button class="choice" onclick={() => run(kind)} disabled={app.readOnly}>
        <Icon size={18} />
        <span class="text">
          <strong>{t(`transforms.${kind}`)}</strong>
          <span class="hint">{t(`transforms.${kind}_hint`)}</span>
        </span>
      </button>
    {/each}
  </div>
  <p class="hint">{t('transforms.note')}</p>
</Dialog>

<style>
  .choices {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .choice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-m);
    background: var(--bg-1);
    text-align: left;
  }
  .choice:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
</style>
