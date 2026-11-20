import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import en from './en.json';
import zh from './zh.json';

export type Lang = 'en' | 'zh';
export type LanguageSetting = 'auto' | Lang;

/** 'auto' follows the browser's preferred language: zh* -> zh, anything else -> en. */
export function pickLanguage(setting: LanguageSetting, navigatorLanguages: readonly string[]): Lang {
  if (setting !== 'auto') return setting;
  const first = navigatorLanguages[0]?.toLowerCase() ?? '';
  return first.startsWith('zh') ? 'zh' : 'en';
}

i18n.on('languageChanged', (language) => { document.documentElement.lang = language; });
void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, zh: { translation: zh } },
  lng: pickLanguage('auto', typeof navigator === 'undefined' ? [] : navigator.languages),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

export default i18n;
