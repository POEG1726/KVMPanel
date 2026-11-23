import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../api/client';
import { useEventsContext } from '../api/EventsProvider';
import { Button, toast } from '../ui';
import { errorText } from './menus/hooks';
import s from './McpActivity.module.css';

export function McpActivity() {
  const { t } = useTranslation();
  const { status, viewOnly } = useEventsContext();
  const qc = useQueryClient();
  const stop = useMutation({
    mutationFn: () => unwrap(api.POST('/api/mcp/stop')),
    onSuccess: () => {
      toast(t('console.mcpStopped'));
      void qc.invalidateQueries({ queryKey: ['mcp'] });
    },
    onError: (error) => toast(errorText(t, error), 'error'),
  });
  const activity = status?.mcp;
  if (!activity?.active) return null;
  return (
    <aside className={s.banner}>
      <span role="status">
        <strong>{t('console.mcpActive')}</strong>
        {activity.key_name && <span className={s.key}>{activity.key_name}</span>}
      </span>
      {!viewOnly && <Button variant="danger" disabled={stop.isPending} onClick={() => stop.mutate()}>{t('console.mcpStop')}</Button>}
    </aside>
  );
}
