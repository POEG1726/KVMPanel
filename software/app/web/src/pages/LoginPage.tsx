import { useEffect, useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, ApiError, errorKey, unwrap } from '../api/client';
import { ME_KEY } from '../session/useSession';

export function LoginPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [errorText, setErrorText] = useState<string | null>(null);
  const [lockedFor, setLockedFor] = useState(0);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (lockedFor <= 0) return;
    const timer = setTimeout(() => setLockedFor((s) => s - 1), 1000);
    return () => clearTimeout(timer);
  }, [lockedFor]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErrorText(null);
    try {
      const { user } = await unwrap(api.POST('/api/auth/login', { body: { username, password } }));
      qc.clear();
      qc.setQueryData(ME_KEY, user);
      navigate('/', { replace: true });
    } catch (err) {
      if (err instanceof ApiError && err.code === 'login_locked') {
        const seconds = Number(err.detail?.retry_after_s);
        if (Number.isFinite(seconds) && seconds > 0) setLockedFor(Math.ceil(seconds));
        else setErrorText(t(errorKey(err)));
      } else {
        setErrorText(err instanceof ApiError ? t(errorKey(err), t('error.unknown')) : t('error.unknown'));
      }
    } finally {
      setBusy(false);
    }
  }

  const locked = lockedFor > 0;
  return (
    <main>
      <h1>{t('login.title')}</h1>
      <form onSubmit={submit}>
        <label>
          {t('login.username')}
          <input autoComplete="username" value={username} onChange={(e) => setUsername(e.target.value)} required />
        </label>
        <label>
          {t('login.password')}
          <input type="password" autoComplete="current-password" value={password} onChange={(e) => setPassword(e.target.value)} />
        </label>
        {locked && <p role="alert">{t('login.locked', { seconds: lockedFor })}</p>}
        {errorText && !locked && <p role="alert">{errorText}</p>}
        <button type="submit" disabled={busy || locked}>
          {t('login.submit')}
        </button>
      </form>
    </main>
  );
}
