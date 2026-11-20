import type { ChangeEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { errorText } from '../../console/menus/hooks';
import i18n, { pickLanguage, type LanguageSetting } from '../../i18n';
import { Select, Switch, toast } from '../../ui';
import { Group, Row, TextField } from '../fields';
import { useConfigSection } from '../useConfigSection';

/** `config.web`: language, how the workspace opens, menu bar behaviour, page title, and the logo. */
export function GeneralPage() {
  const { t } = useTranslation();
  const { value: web, patch } = useConfigSection('web');
  if (!web) return null;

  const setLanguage = async (language: LanguageSetting) => {
    // The language applies at once, not after a reload.
    if (await patch({ language })) void i18n.changeLanguage(pickLanguage(language, navigator.languages));
  };

  const upload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    try {
      await unwrap(api.PUT('/api/branding/logo', { body: file as never, bodySerializer: (b: unknown) => b as never, headers: { 'content-type': file.type } }));
      toast(t('settings.general.logoSaved'));
    } catch (err) {
      toast(errorText(t, err), 'error');
    }
  };

  return (
    <>
      <Group>
        <Row label={t('settings.general.language')}>
          <Select
            label={t('settings.general.language')}
            value={web.language}
            options={[
              { value: 'auto', label: t('settings.general.languageAuto') },
              { value: 'en', label: 'English' },
              { value: 'zh', label: '中文' },
            ]}
            onChange={(v) => void setLanguage(v)}
          />
        </Row>
        <Row label={t('settings.general.workspaceOpen')}>
          <Select
            label={t('settings.general.workspaceOpen')}
            value={web.workspace_open}
            options={[
              { value: 'float', label: t('settings.general.workspaceFloat') },
              { value: 'tab', label: t('settings.general.workspaceTab') },
            ]}
            onChange={(workspace_open) => void patch({ workspace_open })}
          />
        </Row>
        <Row label={t('settings.general.rememberFloat')}>
          <Switch label={t('settings.general.rememberFloat')} checked={web.remember_float} onChange={(remember_float) => void patch({ remember_float })} />
        </Row>
        <Row label={t('settings.general.menubar')}>
          <Select
            label={t('settings.general.menubar')}
            value={web.menubar}
            options={[
              { value: 'auto_hide', label: t('settings.general.menubarAutoHide') },
              { value: 'pinned', label: t('settings.general.menubarPinned') },
            ]}
            onChange={(menubar) => void patch({ menubar })}
          />
        </Row>
      </Group>
      <Group>
        <Row label={t('settings.general.title')}>
          <TextField label={t('settings.general.title')} value={web.title} onCommit={(title) => void patch({ title: title.trim() })} />
        </Row>
        <Row label={t('settings.general.logo')} hint={t('settings.general.logoHint')}>
          <input type="file" aria-label={t('settings.general.logo')} accept="image/png,image/svg+xml" onChange={(e) => void upload(e)} />
        </Row>
      </Group>
    </>
  );
}
