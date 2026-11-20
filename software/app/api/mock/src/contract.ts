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
