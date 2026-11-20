import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Ajv2020, type ValidateFunction } from 'ajv/dist/2020.js';
import addFormatsModule from 'ajv-formats';
import { parse } from 'yaml';

const addFormats = addFormatsModule as unknown as (ajv: Ajv2020) => Ajv2020;

const CONTRACT_PATH = fileURLToPath(new URL('../../openapi.yaml', import.meta.url));
const DOC_ID = 'openapi.json';
const METHODS = ['get', 'put', 'post', 'patch', 'delete'] as const;

type Operation = { responses?: Record<string, { content?: Record<string, { schema?: unknown }> }> };
type OpenApiDoc = {
  info: { version: string };
  paths: Record<string, Partial<Record<(typeof METHODS)[number], Operation & { operationId?: string }>>>;
  components: { schemas: Record<string, unknown> };
};

export const contract: OpenApiDoc = parse(readFileSync(CONTRACT_PATH, 'utf8'));

export const ajv = new Ajv2020({ strict: false, allErrors: true });
addFormats(ajv);
ajv.addSchema(contract, DOC_ID);

const pointerEscape = (s: string) => s.replace(/~/g, '~0').replace(/\//g, '~1');
const opIndex = new Map<string, { path: string; method: string; op: Operation }>();
for (const [path, item] of Object.entries(contract.paths)) {
  for (const method of METHODS) {
    const op = item[method];
    if (op?.operationId) opIndex.set(op.operationId, { path, method, op });
  }
}

const cache = new Map<string, ValidateFunction>();

const loadedMessageSchemas = new Set<string>();

/** Validator for `$defs/<def>` in a message schema file under api/schema/ (e.g. ws.schema.json). */
export function messageValidator(file: string, def: string): ValidateFunction {
  const key = `msg:${file}#${def}`;
  let v = cache.get(key);
  if (!v) {
    if (!loadedMessageSchemas.has(file)) {
      const path = fileURLToPath(new URL(`../../schema/${file}`, import.meta.url));
      ajv.addSchema(JSON.parse(readFileSync(path, 'utf8')));
      loadedMessageSchemas.add(file);
    }
    v = ajv.compile({ $ref: `${file}#/$defs/${pointerEscape(def)}` });
    cache.set(key, v);
  }
  return v;
}

/** Validator for a named schema under components/schemas. */
export function schemaValidator(name: string): ValidateFunction {
  const key = `schema:${name}`;
  let v = cache.get(key);
  if (!v) {
    if (!(name in contract.components.schemas)) throw new Error(`unknown schema ${name}`);
    v = ajv.compile({ $ref: `${DOC_ID}#/components/schemas/${pointerEscape(name)}` });
    cache.set(key, v);
  }
  return v;
}

type RequestOp = Operation & { requestBody?: { content?: Record<string, { schema?: unknown }> } };

/**
 * Validates a JSON request body against the contract. Returns null when valid,
 * otherwise the JSON pointer of the first offending field (for `detail.path`).
 */
export function checkRequest(operationId: string, body: unknown): { path: string } | null {
  const entry = opIndex.get(operationId);
  if (!entry) throw new Error(`unknown operationId ${operationId}`);
  const media = (entry.op as RequestOp).requestBody?.content?.['application/json'];
  if (!media?.schema) return null;
  const key = `req:${operationId}`;
  let v = cache.get(key);
  if (!v) {
    const ptr = ['paths', entry.path, entry.method, 'requestBody', 'content', 'application/json', 'schema']
      .map(pointerEscape)
      .join('/');
    v = ajv.compile({ $ref: `${DOC_ID}#/${ptr}` });
    cache.set(key, v);
  }
  if (v(body)) return null;
  return { path: firstErrorPath(v) };
}

/** JSON pointer of the first Ajv error, pointing at the unknown property itself for additionalProperties. */
export function firstErrorPath(v: ValidateFunction): string {
  const e = v.errors?.[0];
  if (!e) return '';
  if (e.keyword === 'additionalProperties') {
    return `${e.instancePath}/${(e.params as { additionalProperty: string }).additionalProperty}`;
  }
  if (e.keyword === 'required') {
    return `${e.instancePath}/${(e.params as { missingProperty: string }).missingProperty}`;
  }
  return e.instancePath;
}

/** Throws unless `body` matches the JSON response schema of `operationId` for `status`. */
export function validateResponse(operationId: string, status: number, body: unknown): void {
  const entry = opIndex.get(operationId);
  if (!entry) throw new Error(`unknown operationId ${operationId}`);
  const response = entry.op.responses?.[String(status)];
  if (!response) throw new Error(`${operationId}: status ${status} not in contract`);
  const media = response.content?.['application/json'];
  if (!media?.schema) {
    if (body === undefined || body === '' || body === null) return;
    throw new Error(`${operationId}: status ${status} has no JSON body in contract`);
  }
  const key = `${operationId}:${status}`;
  let v = cache.get(key);
  if (!v) {
    const ptr = ['paths', entry.path, entry.method, 'responses', String(status), 'content', 'application/json', 'schema']
      .map(pointerEscape)
      .join('/');
    v = ajv.compile({ $ref: `${DOC_ID}#/${ptr}` });
    cache.set(key, v);
  }
  if (!v(body)) {
    throw new Error(`${operationId} ${status} violates contract: ${ajv.errorsText(v.errors)}`);
  }
}
