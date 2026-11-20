import { useTranslation } from 'react-i18next';

/** Placeholder; Task 4 fills in the video stream and input. */
export function ConsolePage() {
  const { t } = useTranslation();
  return <h1>{t('console.title')}</h1>;
}
