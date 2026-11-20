import { describe, expect, it } from 'vitest';
import i18n, { pickLanguage } from './index';

describe('pickLanguage', () => {
  it('auto: zh-* maps to zh', () => {
    expect(pickLanguage('auto', ['zh-TW'])).toBe('zh');
  });
  it('auto: only the first preferred language counts', () => {
    expect(pickLanguage('auto', ['en-GB', 'zh-CN'])).toBe('en');
  });
  it('auto: unknown languages map to en', () => {
    expect(pickLanguage('auto', ['fr'])).toBe('en');
  });
  it('explicit setting wins over navigator', () => {
    expect(pickLanguage('zh', ['en'])).toBe('zh');
  });
});

describe('i18n fallback', () => {
  it('renders zh strings when available', async () => {
    await i18n.changeLanguage('zh');
    expect(i18n.t('login.title')).not.toBe('login.title');
    expect(i18n.t('login.title')).not.toBe(i18n.getFixedT('en')('login.title'));
  });
  it('falls back to English for keys only in en.json', async () => {
    await i18n.changeLanguage('zh');
    expect(i18n.t('dev.enOnly')).toBe('English only');
  });
});
