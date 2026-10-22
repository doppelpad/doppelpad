// A bindings sheet: every layer's mappings and analog routes as one printable HTML page.
import { t } from '../i18n/index.svelte';
import type { PadFamily, Profile } from '../types';
import { sourceOf, targetOf } from './analog';
import { comboLabel, layerLabel } from './labels';
import { comboKey, layerIds, mappingSummary } from './profile';

const escape = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

const STYLE = `
  body { font: 13px/1.45 system-ui, sans-serif; color: #111; margin: 24px; }
  h1 { font-size: 20px; margin: 0 0 4px; }
  h2 { font-size: 15px; margin: 22px 0 6px; border-bottom: 1px solid #999; padding-bottom: 2px; }
  p.hint { color: #555; margin: 0 0 8px; }
  table { border-collapse: collapse; width: 100%; }
  td { padding: 3px 8px 3px 0; vertical-align: top; border-bottom: 1px solid #ddd; }
  td.control { width: 30%; font-weight: 600; }
  td.note { color: #555; width: 30%; }
  @media print { body { margin: 0; } h2 { break-after: avoid; } tr { break-inside: avoid; } }
`;

/** The sheet for `p`, with control names for `family`. */
export function bindingsSheet(p: Profile, family: PadFamily | null): string {
  const label = (id: number) => layerLabel(p, id);
  const sections = layerIds(p).flatMap((id) => {
    const mappings = p.mappings.filter((m) => (m.layer ?? 0) === id);
    const routes = (p.analog ?? []).filter((r) => (r.layer ?? 0) === id);
    if (mappings.length === 0 && routes.length === 0) return [];
    // One row per trigger, in the order the config lists them.
    const rows = new Map<string, { control: string; action: string[]; note: string[] }>();
    for (const m of mappings) {
      const key = comboKey(m.trigger);
      const row = rows.get(key) ?? {
        control: comboLabel(m.trigger, family),
        action: [],
        note: [],
      };
      row.action.push(mappingSummary(m, label));
      if (m.description) row.note.push(m.description);
      rows.set(key, row);
    }
    const body = [...rows.values()]
      .map(
        (r) =>
          `<tr><td class="control">${escape(r.control)}</td><td>${escape(r.action.join(' · '))}</td>` +
          `<td class="note">${escape(r.note.join(' · '))}</td></tr>`,
      )
      .join('\n');
    const routeRows = routes
      .map(
        (r) =>
          `<tr><td class="control">${escape(t(`analog.routes.sources.${sourceOf(r)}`))}</td>` +
          `<td>→ ${escape(t(`analog.routes.targets.${targetOf(r)}`))}</td><td class="note"></td></tr>`,
      )
      .join('\n');
    return [`<h2>${escape(label(id))}</h2>\n<table>\n${body}\n${routeRows}\n</table>`];
  });
  const title = escape(p.name || t('sheet.untitled'));
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>${title}</title><style>${STYLE}</style></head>
<body>
<h1>${title}</h1>
${p.description ? `<p class="hint">${escape(p.description)}</p>` : ''}
${sections.join('\n') || `<p class="hint">${escape(t('sheet.empty'))}</p>`}
</body></html>
`;
}
