import { useTranslation } from 'react-i18next';

/** Placeholder for the workspace route. */
export function WorkspacePage() {
  const { t } = useTranslation();
  return <h1>{t('workspace.title')}</h1>;
}
