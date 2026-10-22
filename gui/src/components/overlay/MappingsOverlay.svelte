<script lang="ts">
  // The running profile's mappings on the active layer, sent by the app (pad_overlay.rs, see
  // lib/overlay.ts): one row per trigger, in columns of up to 40 rows.
  import { onMount } from 'svelte';
  import { setLanguage, t } from '../../lib/i18n/index.svelte';
  import { layerLabel, overlayRows } from '../../lib/model/labels';
  import { overlayContent } from '../../lib/overlay';
  import type { MappingsOverlayContent } from '../../lib/types';
  import CornerFrame from './CornerFrame.svelte';

  let content = $state<MappingsOverlayContent | null>(null);

  onMount(() =>
    overlayContent<MappingsOverlayContent>((c) => {
      if (c) setLanguage(c.lang);
      content = c;
    }),
  );

  const ROWS = 40;
  const MAX = ROWS * 3;

  const rows = $derived(content ? overlayRows(content.profile, content.layer, content.family) : []);
  const columns = $derived(
    Array.from({ length: Math.ceil(Math.min(rows.length, MAX) / ROWS) }, (_, i) => rows.slice(i * ROWS, (i + 1) * ROWS)),
  );
</script>

{#if content}
  <CornerFrame corner={content.corner} opacity={content.opacity}>
    <div class="sheet">
      <div class="head">
        <span class="title">{content.profile.name}</span>
        <span class="layer">{layerLabel(content.profile, content.layer)}</span>
      </div>
      {#if rows.length === 0}
        <p class="empty">{t('overlays.no_mappings')}</p>
      {:else}
        <div class="cols">
          {#each columns as col, ci (ci)}
            <table>
              <tbody>
                {#each col as r, ri (ri)}
                  <tr>
                    <td class="ctl">{r.control}</td>
                    <td class="act"
                      >{r.action}{#if r.note}<span class="note"> — {r.note}</span>{/if}</td
                    >
                  </tr>
                {/each}
              </tbody>
            </table>
          {/each}
        </div>
        {#if rows.length > MAX}<p class="more">{t('overlays.more', { n: rows.length - MAX })}</p>{/if}
      {/if}
    </div>
  </CornerFrame>
{/if}

<style>
  .sheet {
    max-width: 100%;
    max-height: 100%;
    overflow: hidden;
    padding: 12px 14px;
    border-radius: var(--radius-l);
    background: color-mix(in srgb, var(--bg-0) 90%, transparent);
    border: 1px solid var(--line-strong);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 8px;
  }
  .title {
    font-size: 15px;
    font-weight: 600;
  }
  .layer {
    color: var(--accent);
    font-size: var(--fs-s);
    font-weight: 600;
  }
  .cols {
    display: flex;
    gap: 18px;
    align-items: flex-start;
  }
  table {
    border-collapse: collapse;
    width: 360px;
    table-layout: fixed;
  }
  td {
    padding: 2px 6px 2px 0;
    font-size: var(--fs-s);
    line-height: 18px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ctl {
    width: 130px;
    color: var(--text-dim);
    font-weight: 600;
  }
  .note {
    color: var(--text-faint);
  }
  .empty,
  .more {
    margin: 4px 0 0;
    color: var(--text-dim);
    font-size: var(--fs-s);
  }
</style>
