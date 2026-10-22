// What the binding panel's sections share.
import { parseNonNegInt } from '../../lib/model/numbers';
import { app } from '../../lib/state/app.svelte';
import type { Mapping } from '../../lib/types';

/** Applies a change to the selected mapping (`app.editMapping`); `keepEmpty` keeps it when it ends up empty. */
export type EditMapping = (change: (m: Mapping) => void, keepEmpty?: boolean) => void;

/** Timing a mapping can set for itself. */
export type MappingTiming = 'turbo_ms' | 'long_press_ms' | 'double_tap_ms' | 'triple_tap_ms' | 'key_delay_ms';

/** A typed time: a whole number of ms, or nothing for "not set". */
export const ms = (v: string) => parseNonNegInt(v) ?? undefined;

/** The config's timing over the defaults: what applies when a mapping sets none. */
export const configTiming = () => ({ ...app.catalog?.defaults.timing, ...(app.profile?.timing ?? {}) });
