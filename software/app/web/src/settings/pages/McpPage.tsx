import { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { useRun } from '../../console/menus/hooks';
import { Button, ConfirmDialog, Switch, toast } from '../../ui';
import { Alert, Group, Row } from '../fields';
import s from '../settings.module.css';

const MCP_KEY = ['mcp'] as const;

/**
 * The full key exists only in the answer to "regenerate". It lives in this component's state and
 * nowhere else, so leaving the page (or reloading) leaves just the last four characters.
 */
export function McpPage() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const mcp = useQuery({ queryKey: MCP_KEY, queryFn: () => unwrap(api.GET('/api/mcp')) });
  const { error, run } = useRun();
  const [fresh, setFresh] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<'regenerate' | 'revoke' | null>(null);
  const refresh = () => qc.invalidateQueries({ queryKey: MCP_KEY });

  const setEnabled = (enabled: boolean) =>
    run(async () => {
      qc.setQueryData(MCP_KEY, await unwrap(api.PUT('/api/mcp', { body: { enabled } })));
    });
  const regenerate = () =>
    run(async () => {
      setFresh((await unwrap(api.POST('/api/mcp/regenerate'))).key);
      await refresh();
    });
  const revoke = () =>
    run(async () => {
      await unwrap(api.DELETE('/api/mcp/key'));
      setFresh(null);
      await refresh();
    });
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(fresh ?? '');
      toast(t('settings.mcp.copied'));
    } catch {
      toast(t('settings.mcp.copyFailed'), 'error');
    }
  };

  const hasKey = !!mcp.data?.key_hint;
  return (
    <>
      <Group>
        <Row label={t('settings.mcp.enabled')} hint={t('settings.mcp.enabledHint')}>
          <Switch label={t('settings.mcp.enabled')} checked={!!mcp.data?.enabled} disabled={!mcp.data} onChange={(on) => void setEnabled(on)} />
        </Row>
        <Row label={t('settings.mcp.key')}>
          <span className={s.mono}>{hasKey ? t('settings.mcp.keyHint', { hint: mcp.data?.key_hint }) : t('settings.mcp.noKey')}</span>
        </Row>
      </Group>
      {fresh && (
        <div className={s.secret}>
          <p>{t('settings.mcp.showOnce')}</p>
          <code className={s.mono} data-testid="mcp-key">
            {fresh}
          </code>
          <div className={s.actions}>
            <Button onClick={() => void copy()}>{t('settings.mcp.copy')}</Button>
            <Button onClick={() => setFresh(null)}>{t('settings.mcp.hide')}</Button>
          </div>
        </div>
      )}
      <div className={s.actions}>
        <Button variant="primary" onClick={() => (hasKey ? setConfirm('regenerate') : void regenerate())}>
          {t('settings.mcp.regenerate')}
        </Button>
        {hasKey && <Button onClick={() => setConfirm('revoke')}>{t('settings.mcp.revoke')}</Button>}
      </div>
      <Alert message={error} />
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
            void (what === 'regenerate' ? regenerate() : revoke());
          }}
        />
      )}
    </>
  );
}
