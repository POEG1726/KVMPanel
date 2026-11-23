import { useRef, useState, type FormEvent } from 'react';
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import type { components } from '../../api/schema';
import { errorText } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Switch, toast } from '../../ui';
import { Alert, Group, Row } from '../fields';
import s from '../settings.module.css';

export const MCP_KEY = ['mcp'] as const;
const AUDIT_KEY = ['mcp-audit'] as const;
type Scope = components['schemas']['McpScope'];
type Key = components['schemas']['McpKey'];
type Draft = components['schemas']['McpKeyEdit'] & { id?: string };
type InputPolicy = Partial<Pick<components['schemas']['McpState'], 'human_input_priority' | 'human_input_wait_s'>>;
type Action = { enabled: boolean } | { inputPolicy: InputPolicy } | { save: Draft } | { remove: string };
const scopes: Scope[] = ['observe', 'input', 'power', 'media', 'host_private'];

export function McpPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const mcp = useQuery({ queryKey: MCP_KEY, queryFn: () => unwrap(api.GET('/api/mcp')) });
  const audit = useQuery({ queryKey: AUDIT_KEY, queryFn: () => unwrap(api.GET('/api/mcp/audit', { params: { query: { limit: 100 } } })), refetchInterval: 8000 });
  const [fresh, setFresh] = useState<{ id: string; key: string } | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [remove, setRemove] = useState<Key | null>(null);
  const keyElement = useRef<HTMLElement>(null);
  const changing = useIsMutating({ mutationKey: MCP_KEY }) > 0;
  const mutation = useMutation({
    mutationKey: MCP_KEY,
    mutationFn: async (action: Action) => {
      if ('enabled' in action) {
        qc.setQueryData(MCP_KEY, await unwrap(api.PUT('/api/mcp', { body: action })));
      } else if ('inputPolicy' in action) {
        qc.setQueryData(MCP_KEY, await unwrap(api.PATCH('/api/mcp', { body: action.inputPolicy })));
      } else if ('remove' in action) {
        await unwrap(api.DELETE('/api/mcp/keys/{id}', { params: { path: { id: action.remove } } }));
        setFresh((current) => current?.id === action.remove ? null : current);
      } else {
        const { id, ...body } = action.save;
        if (id) await unwrap(api.PATCH('/api/mcp/keys/{id}', { params: { path: { id } }, body }));
        else {
          const { key, entry } = await unwrap(api.POST('/api/mcp/keys', { body }));
          setFresh({ id: entry.id, key });
        }
        setDraft(null);
      }
    },
    onSettled: () => Promise.all([qc.invalidateQueries({ queryKey: MCP_KEY }), qc.invalidateQueries({ queryKey: AUDIT_KEY })]),
  });
  const ready = mcp.isSuccess && !mcp.isFetching && !changing;
  const change = (action: Action) => {
    if (ready && !qc.isMutating({ mutationKey: MCP_KEY })) mutation.mutate(action);
  };
  const save = (event: FormEvent) => {
    event.preventDefault();
    if (draft?.name.trim()) change({ save: { ...draft, name: draft.name.trim() } });
  };
  const copy = async () => {
    if (!fresh || !keyElement.current) return;
    try {
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(fresh.key);
        toast(t('settings.mcp.copied'));
        return;
      }
    } catch { /* Select the key for browsers that block clipboard access. */ }
    if (!keyElement.current) return;
    const selection = window.getSelection();
    const range = document.createRange();
    range.selectNodeContents(keyElement.current);
    selection?.removeAllRanges();
    selection?.addRange(range);
    try {
      if (document.execCommand?.('copy')) {
        toast(t('settings.mcp.copied'));
        return;
      }
    } catch { /* Leave the key selected for the browser's Copy command. */ }
    toast(t('settings.mcp.copyFailed'), 'error');
  };

  const keys = mcp.data?.keys ?? [];
  const wait = mcp.data?.human_input_wait_s;
  const endpoint = `${window.location.origin}/api/mcp/v1`;
  const exampleKey = fresh?.key ?? '<key>';
  return (
    <>
      <Group>
        <Row label={t('settings.mcp.enabled')} hint={t('settings.mcp.enabledHint')}>
          <Switch label={t('settings.mcp.enabled')} checked={!!mcp.data?.enabled}
            disabled={!ready || (!mcp.data?.enabled && !keys.length)} onChange={(enabled) => change({ enabled })} />
        </Row>
        <Row label={t('settings.mcp.humanPriority')} hint={t('settings.mcp.humanPriorityHint')}>
          <Switch label={t('settings.mcp.humanPriority')} checked={mcp.data?.human_input_priority ?? true}
            disabled={!ready} onChange={(checked) => change({ inputPolicy: { human_input_priority: checked } })} />
        </Row>
        <Row label={t('settings.mcp.humanWait')} hint={t('settings.mcp.humanWaitHint')}>
          <input key={wait} className={s.input} type="number" aria-label={t('settings.mcp.humanWait')}
            min={1} max={60} step={1} required defaultValue={wait} disabled={!ready}
            onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }}
            onBlur={({ currentTarget: field }) => {
              const seconds = field.valueAsNumber;
              const valid = field.validity.valid;
              field.value = wait === undefined ? '' : String(wait);
              if (valid && seconds !== wait) change({ inputPolicy: { human_input_wait_s: seconds } });
            }} />
        </Row>
      </Group>
      <Group title={t('settings.mcp.keys')}>
        {mcp.isSuccess && !keys.length && <p className={s.muted}>{t('settings.mcp.noKey')}</p>}
        {keys.map((key) => (
          <Row key={key.id} label={key.name} hint={[t('settings.mcp.keyHint', { hint: key.hint }),
            ...key.scopes.map((scope) => t(`settings.mcp.scope.${scope}`))].join(' · ')}>
            <Button disabled={!ready} onClick={() => setDraft({ id: key.id, name: key.name, scopes: key.scopes })}>{t('settings.mcp.edit')}</Button>
            <Button disabled={!ready} onClick={() => setRemove(key)}>{t('settings.mcp.revoke')}</Button>
          </Row>
        ))}
        <div className={s.actions}>
          <Button variant="primary" disabled={!ready || keys.length >= 8}
            onClick={() => setDraft({ name: '', scopes: ['observe'] })}>{t('settings.mcp.create')}</Button>
        </div>
        {draft && (
          <form className={s.mcpForm} onSubmit={save}>
            <label>{t('settings.mcp.name')}
              <input className={s.input} value={draft.name} maxLength={64} required
                onChange={(event) => setDraft({ ...draft, name: event.target.value })} />
            </label>
            <fieldset className={s.checks}>
              <legend>{t('settings.mcp.scopes')}</legend>
              {scopes.map((scope) => (
                <label key={scope}>
                  <input type="checkbox" checked={draft.scopes.includes(scope)} onChange={(event) =>
                    setDraft({ ...draft, scopes: event.target.checked
                      ? [...draft.scopes, scope] : draft.scopes.filter((item) => item !== scope) })} />
                  {t(`settings.mcp.scope.${scope}`)}
                </label>
              ))}
            </fieldset>
            <div className={s.actions}>
              <Button type="submit" variant="primary" disabled={!ready || !draft.name.trim()}>{t('settings.mcp.save')}</Button>
              <Button onClick={() => setDraft(null)}>{t('common.cancel')}</Button>
            </div>
          </form>
        )}
        {fresh && (
          <div className={s.secret}>
            <p>{t('settings.mcp.showOnce')}</p>
            <code ref={keyElement} className={s.mono} data-testid="mcp-key">{fresh.key}</code>
            <div className={s.actions}>
              <Button onClick={() => void copy()}>{t('settings.mcp.copy')}</Button>
              <Button onClick={() => setFresh(null)}>{t('settings.mcp.hide')}</Button>
            </div>
          </div>
        )}
        <Alert message={mutation.error || mcp.error ? errorText(t, mutation.error ?? mcp.error) : null} />
      </Group>
      <Group title={t('settings.mcp.clients')}>
        <p>Claude Code</p>
        <pre className={s.snippet}>{`claude mcp add --transport http kvmpanel ${endpoint} --header "Authorization: Bearer ${exampleKey}"`}</pre>
        <p>OpenCode</p>
        <pre className={s.snippet}>{JSON.stringify({ mcp: { kvmpanel: { type: 'remote', url: endpoint, headers: { Authorization: `Bearer ${exampleKey}` } } } }, null, 2)}</pre>
        <p className={s.muted}>{t('settings.mcp.certificate')}</p>
      </Group>
      <Group title={t('settings.mcp.audit')}>
        <Alert message={audit.error ? errorText(t, audit.error) : null} />
        {audit.isSuccess && !audit.data.entries.length && <p className={s.muted}>{t('settings.mcp.emptyAudit')}</p>}
        {!!audit.data?.entries.length && (
          <table className={s.table}>
            <thead><tr>{['time', 'keys', 'operation', 'outcome'].map((name) => <th key={name}>{t(`settings.mcp.${name}`)}</th>)}</tr></thead>
            <tbody>{audit.data.entries.map((entry, index) => (
              <tr key={`${entry.ts}-${index}`}>
                <td>{new Date(entry.ts).toLocaleString()}</td>
                <td>{entry.key_name ?? '—'}</td>
                <td><code>{entry.tool}</code>{entry.count > 1 && ` ×${entry.count}`}</td>
                <td>{entry.result} · {entry.elapsed_ms} ms</td>
              </tr>
            ))}</tbody>
          </table>
        )}
      </Group>
      {remove && (
        <ConfirmDialog title={t('settings.mcp.revokeTitle')} message={t('settings.mcp.revokeMessage', { name: remove.name })}
          confirmLabel={t('settings.mcp.revoke')} danger onCancel={() => setRemove(null)}
          onConfirm={() => { change({ remove: remove.id }); setRemove(null); }} />
      )}
    </>
  );
}
