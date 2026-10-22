<script lang="ts">
  // The list of the editor's keyboard shortcuts, from the same table the key handler uses.
  import { t } from '../lib/i18n/index.svelte';
  import { SHORTCUTS, keysLabel, type Keys } from '../lib/state/shortcuts';
  import { platform } from '../lib/state/window.svelte';
  import Dialog from './ui/Dialog.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const mac = platform === 'macos';

  /** The layer digits read as one range: "Ctrl+1 … Ctrl+9". */
  function keysText(keys: Keys[]): string[] {
    if (keys.length > 2 && keys.every((k) => /^\d$/.test(k.key))) return [`${keysLabel(keys[0]!, mac)} … ${keysLabel(keys.at(-1)!, mac)}`];
    return keys.map((k) => keysLabel(k, mac));
  }
</script>

<Dialog title={t('shortcuts_list.title')} {onclose} width={480}>
  <table>
    <tbody>
      {#each SHORTCUTS as s (s.id)}
        <tr>
          <td>{t(`shortcuts_list.${s.id}`)}</td>
          <td class="keys">
            {#each keysText(s.keys) as k, i (i)}
              {#if i > 0}<span class="or">{t('shortcuts_list.or')}</span>{/if}
              <kbd>{k}</kbd>
            {/each}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="hint">{t('shortcuts_list.hint')}</p>
</Dialog>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
  }
  td {
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }
  .keys {
    text-align: right;
    white-space: nowrap;
  }
  kbd {
    display: inline-block;
    padding: 2px 6px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-s);
    background: var(--bg-2);
    font-family: inherit;
    font-size: var(--fs-s);
  }
  .or {
    margin: 0 6px;
    color: var(--text-faint);
    font-size: var(--fs-s);
  }
</style>
