import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useQueryClient } from '@tanstack/react-query';
import { api, ApiError, errorKey, unwrap } from '../api/client';
import { CONFIG_KEY, ME_KEY } from './useSession';

const MIN_LENGTH = 6;

/** Modal that cannot be dismissed: no close button, Escape and backdrop do nothing. */
export function ChangePasswordDialog() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [old, setOld] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (next.length < MIN_LENGTH) return setError(t('changePassword.tooShort', { min: MIN_LENGTH }));
    if (next !== confirm) return setError(t('changePassword.mismatch'));
    setBusy(true);
    setError(null);
    try {
      await unwrap(api.POST('/api/users/me/password', { body: { old, new: next } }));
      await qc.invalidateQueries({ queryKey: CONFIG_KEY });
      await qc.invalidateQueries({ queryKey: ME_KEY });
    } catch (err) {
      setError(err instanceof ApiError ? t(errorKey(err), t('error.unknown')) : t('error.unknown'));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop">
      <form role="dialog" aria-modal="true" aria-labelledby="cp-title" onSubmit={submit} onKeyDown={(e) => e.key === 'Escape' && e.preventDefault()}>
        <h2 id="cp-title">{t('changePassword.title')}</h2>
        <p>{t('changePassword.hint')}</p>
        <label>
          {t('changePassword.old')}
          <input type="password" autoComplete="current-password" value={old} onChange={(e) => setOld(e.target.value)} required />
        </label>
        <label>
          {t('changePassword.new')}
          <input type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} required />
        </label>
        <label>
          {t('changePassword.confirm')}
          <input type="password" autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} required />
        </label>
        {error && <p role="alert">{error}</p>}
        <button type="submit" disabled={busy}>
          {t('changePassword.submit')}
        </button>
      </form>
    </div>
  );
}
