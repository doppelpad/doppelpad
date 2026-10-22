// Typed fixtures for the tests: Rust's defaults as the catalog command sends them, the zone
// defaults made of them, and an empty profile.
import type { Catalog, Profile } from '../../types';
import { zoneDefaults as zoneDefaultsOf } from '../zones';
import defaultsJson from './defaults.json';

// Through `unknown`: JSON imports type arrays as `number[]`, the model uses tuples.
export const defaults = defaultsJson as unknown as Catalog['defaults'];
export const zoneDefaults = zoneDefaultsOf(defaults);
export const emptyProfile = (): Profile => ({ name: 'Test', mappings: [] });
