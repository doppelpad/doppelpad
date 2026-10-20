<script lang="ts">
  // The running profile's mappings on the active layer, sent by the app (pad_overlay.rs, see
  // lib/overlay.ts): one row per trigger, in columns of up to 40 rows.
  import { onMount } from 'svelte';
  import { setLanguage, t } from '../../lib/i18n/index.svelte';
  import { controlLabel } from '../../lib/model/controls';
  import { comboKey, comboList, effectiveMappings, layerName, mappingSummary } from '../../lib/model/profile';
  import { overlayContent } from '../../lib/overlay';
  import type { Combo, Corner, PadFamily, Profile } from '../../lib/types';

  type Content = { profile: Profile; layer: number; family: PadFamily | null; lang: string; opacity: number; corner: Corner };
  let content = $state<Content | null>(null);

  onMount(() =>
    overlayContent<Content>((c) => {
      if (c) setLanguage(c.lang);
      content = c;
    }),
  );

  const ROWS = 40;
  const MAX = ROWS * 3;

  type Row = { control: string; action: string; note: string };
  const rows = $derived.by((): Row[] => {
    if (!content) return [];
    const { profile, layer, family } = content;
    const label = (id: number) => layerName(profile, id, t('layers.main'), (n) => t('layers.shift', { n }));
    // Triggers of this layer first, then the base layer's that it takes over.
    const seen = new Set<string>();
    const triggers: Combo[] = [];
    const own = profile.mappings.filter((m) => (m.layer ?? 0) === layer);
    const base = profile.mappings.filter((m) => (m.layer ?? 0) === 0);
    for (const m of [...own, ...base]) {
      const k = comboKey(m.trigger);
      if (!seen.has(k)) {
        seen.add(k);
        triggers.push(m.trigger);
      }
    }
    return triggers.flatMap((trigger) => {
      const eff = effectiveMappings(profile, layer, trigger);
      if (eff.mappings.length === 0) return [];
      return [
        {
          control: comboList(trigger)
            .map((c) => controlLabel(c, family))
            .join(' + '),
          action: eff.mappings.map((m) => mappingSummary(m, label)).join(' · '),
          note: eff.mappings
            .map((m) => m.description ?? '')
            .filter(Boolean)
            .join(' · '),
        },
      ];
    });
  });
  const columns = $derived(
    Array.from({ length: Math.ceil(Math.min(rows.length, MAX) / ROWS) }, (_, i) => rows.slice(i * ROWS, (i + 1) * ROWS)),
  );
</script>

{#if content}
  <div
    class="frame"
    class:right={content.corner.endsWith('right')}
    class:bottom={content.corner.startsWith('bottom')}
    style:opacity={content.opacity / 100}
  >
    <div class="sheet">
      <div class="head">
        <span class="title">{content.profile.name}</span>
        <span class="layer">{layerName(content.profile, content.layer, t('layers.main'), (n) => t('layers.shift', { n }))}</span>
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
                    <td class="act">{r.action}{#if r.note}<span class="note"> — {r.note}</span>{/if}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/each}
        </div>
        {#if rows.length > MAX}<p class="more">{t('overlays.more', { n: rows.length - MAX })}</p>{/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .frame {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: flex-start;
    justify-content: flex-start;
    padding: 6px;
  }
  .frame.right {
    justify-content: flex-end;
  }
  .frame.bottom {
    align-items: flex-end;
  }
  .sheet {
    max-width: 100%;
    max-height: 100%;
    overflow: hidden;
    padding: 12px 14px;
    border-radius: var(--radius-l);
    background: rgba(15, 17, 21, 0.9);
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
    font-size: 12px;
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
    font-size: 12px;
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
    font-size: 12px;
  }
</style>
