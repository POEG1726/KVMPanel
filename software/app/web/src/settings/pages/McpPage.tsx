import { useRef, useState } from 'react';
import { useIsMutating, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Switch, toast } from '../../ui';
import { Alert, Group, Row } from '../fields';
import s from '../settings.module.css';

const MCP_KEY = ['mcp'] as const;
type Action = 'regenerate' | 'revoke' | boolean;

/**
 * The full key exists only in the answer to "regenerate". It lives in this component's state and
 * nowhere else, so leaving the page (or reloading) leaves just the last four characters.
 */
export function McpPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const mcp = useQuery({ queryKey: MCP_KEY, queryFn: () => unwrap(api.GET('/api/mcp')) });
  const [fresh, setFresh] = useState<string | null>(null);
  const keyElement = useRef<HTMLElement>(null);
  const [confirm, setConfirm] = useState<'regenerate' | 'revoke' | null>(null);
  const changing = useIsMutating({ mutationKey: MCP_KEY }) > 0;
  const mutation = useMutation({
    mutationKey: MCP_KEY,
    mutationFn: async (action: Action) => {
      if (action === 'regenerate') {
        const { key } = await unwrap(api.POST('/api/mcp/regenerate'));
        setFresh(key);
        qc.setQueryData(MCP_KEY, { enabled: true, key_hint: key.slice(-4) });
      } else if (action === 'revoke') {
        await unwrap(api.DELETE('/api/mcp/key'));
        setFresh(null);
        qc.setQueryData(MCP_KEY, { enabled: false, key_hint: null });
      } else {
        qc.setQueryData(MCP_KEY, await unwrap(api.PUT('/api/mcp', { body: { enabled: action } })));
      }
    },
    onSettled: () => qc.invalidateQueries({ queryKey: MCP_KEY }),
  });
  const ready = mcp.isSuccess && !mcp.isFetching && !changing;
  const change = (action: Action) => {
    if (ready && !qc.isMutating({ mutationKey: MCP_KEY })) mutation.mutate(action);
  };
  const copy = async () => {
    if (!fresh || !keyElement.current) return;
    try {
      if (navigator.clipboard) {
        await navigator.clipboard.writeText(fresh);
        toast(t('settings.mcp.copied'));
        return;
      }
    } catch { /* A browser may deny clipboard access even on HTTPS. */ }
    // The browser's Copy command works for a user-selected key on a plain HTTP LAN.
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
    } catch { /* Leave the key selected for the user's own Copy action. */ }
    toast(t('settings.mcp.copyFailed'), 'error');
  };

  const hasKey = !!mcp.data?.key_hint;
  return (
    <>
      <Group>
        <Row label={t('settings.mcp.enabled')} hint={t('settings.mcp.enabledHint')}>
          <Switch label={t('settings.mcp.enabled')} checked={!!mcp.data?.enabled} disabled={!ready} onChange={change} />
        </Row>
        <Row label={t('settings.mcp.key')}>
          <span className={s.mono}>{hasKey ? t('settings.mcp.keyHint', { hint: mcp.data?.key_hint }) : mcp.isSuccess ? t('settings.mcp.noKey') : '—'}</span>
        </Row>
      </Group>
      {fresh && (
        <div className={s.secret}>
          <p>{t('settings.mcp.showOnce')}</p>
          <code ref={keyElement} className={s.mono} data-testid="mcp-key">
            {fresh}
          </code>
          <div className={s.actions}>
            <Button onClick={() => void copy()}>{t('settings.mcp.copy')}</Button>
            <Button onClick={() => setFresh(null)}>{t('settings.mcp.hide')}</Button>
          </div>
        </div>
      )}
      <div className={s.actions}>
        <Button variant="primary" disabled={!ready} onClick={() => (hasKey ? setConfirm('regenerate') : change('regenerate'))}>
          {t('settings.mcp.regenerate')}
        </Button>
        {hasKey && <Button disabled={!ready} onClick={() => setConfirm('revoke')}>{t('settings.mcp.revoke')}</Button>}
      </div>
      <Alert message={mutation.error || mcp.error ? errorText(t, mutation.error ?? mcp.error) : null} />
      {confirm && (
        <ConfirmDialog
          title={t(`settings.mcp.${confirm}Title`)}
          message={t(`settings.mcp.${confirm}Message`)}
          confirmLabel={t(`settings.mcp.${confirm}`)}
          danger
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const what = confirm;
            setConfirm(null);
            change(what);
          }}
        />
      )}
    </>
  );
}
