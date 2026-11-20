import { contract } from './contract.js';

export type Config = Record<string, Record<string, unknown>>;

type JsonSchema = { properties?: Record<string, JsonSchema>; default?: unknown };

/** Builds the default config from the `default` values in the Config schema of openapi.yaml. */
export function defaultConfig(): Config {
  const root = contract.components.schemas.Config as JsonSchema;
  const out: Config = {};
  for (const [section, schema] of Object.entries(root.properties ?? {})) {
    out[section] = {};
    for (const [field, fs] of Object.entries(schema.properties ?? {})) {
      out[section][field] = structuredClone(fs.default);
    }
  }
  return out;
}
