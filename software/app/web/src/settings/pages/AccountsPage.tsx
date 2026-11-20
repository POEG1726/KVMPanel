import { useState, type FormEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, unwrap } from '../../api/client';
import { beginOwnPasswordChange, endOwnPasswordChange, settleOwnPasswordChange } from '../../api/authEvents';
import type { components } from '../../api/schema';
import { useRun } from '../../console/menus/hooks';
import { ME_KEY } from '../../session/useSession';
import { Button, ConfirmDialog, Select, Switch, toast } from '../../ui';
import { Alert, Group, Row } from '../fields';
import s from '../settings.module.css';

type User = components['schemas']['User'];
type Permission = components['schemas']['Permission'];
type Role = components['schemas']['Role'];

export const PERMISSIONS: readonly Permission[] = ['console', 'power', 'media', 'terminal', 'files', 'device', 'network', 'updates', 'mcp', 'tailscale', 'panel', 'usb'];
const USERS_KEY = ['users'] as const;
export const MIN_PASSWORD = 6;

function PermissionChecks({ name, selected, disabled, onChange }: { name: string; selected: readonly Permission[]; disabled?: boolean; onChange(next: Permission[]): void }) {
  const { t } = useTranslation();
  return (
    <div className={s.checks}>
      {PERMISSIONS.map((p) => (
        <label key={p}>
          <input
            type="checkbox"
            aria-label={`${name}: ${t(`settings.accounts.perm.${p}`)}`}
            checked={selected.includes(p)}
            disabled={disabled}
            onChange={(e) => onChange(e.target.checked ? [...selected, p] : selected.filter((x) => x !== p))}
          />
          {t(`settings.accounts.perm.${p}`)}
        </label>
      ))}
    </div>
  );
}

/** Changing your own password ends all your sessions; the server re-issues this browser's cookie. */
function OwnPassword() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const { error, run, clear } = useRun();
  const [old, setOld] = useState('');
  const [next, setNext] = useState('');
  const [confirm, setConfirm] = useState('');
  const [local, setLocal] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    clear();
    if (next.length < MIN_PASSWORD) return setLocal(t('changePassword.tooShort', { min: MIN_PASSWORD }));
    if (next !== confirm) return setLocal(t('changePassword.mismatch'));
    setLocal(null);
    void run(async () => {
      // The 4401 that follows on our own sockets must reconnect them instead of leaving for /login.
      beginOwnPasswordChange();
      try {
        await unwrap(api.POST('/api/users/me/password', { body: { old, new: next } }));
      } catch (err) {
        endOwnPasswordChange();
        throw err;
      }
      settleOwnPasswordChange();
      setOld('');
      setNext('');
      setConfirm('');
      await qc.invalidateQueries({ queryKey: ME_KEY });
      toast(t('settings.accounts.passwordChanged'));
    });
  };

  const field = (label: string, value: string, set: (v: string) => void, autoComplete: string) => (
    <Row label={label}>
      <input className={s.input} type="password" aria-label={label} autoComplete={autoComplete} value={value} onChange={(e) => set(e.target.value)} />
    </Row>
  );
  return (
    <form onSubmit={submit}>
      <Group title={t('settings.accounts.ownPassword')}>
        {field(t('changePassword.old'), old, setOld, 'current-password')}
        {field(t('changePassword.new'), next, setNext, 'new-password')}
        {field(t('changePassword.confirm'), confirm, setConfirm, 'new-password')}
        <Alert message={local ?? error} />
        <div className={s.actions}>
          <Button type="submit" variant="primary">
            {t('changePassword.submit')}
          </Button>
        </div>
      </Group>
    </form>
  );
}

function UserRow({ user, onChange, onDelete }: { user: User; onChange(part: Record<string, unknown>): void; onDelete(): void }) {
  const { t } = useTranslation();
  const [password, setPassword] = useState('');
  const isAdmin = user.role === 'admin';
  return (
    <tr>
      <td>{user.name}</td>
      <td>
        <Select
          label={`${user.name}: ${t('settings.accounts.role')}`}
          value={user.role}
          options={[
            { value: 'admin', label: t('settings.accounts.roleAdmin') },
            { value: 'user', label: t('settings.accounts.roleUser') },
          ]}
          onChange={(role) => onChange({ role })}
        />
      </td>
      <td>
        {isAdmin ? <span className={s.muted}>{t('settings.accounts.allPermissions')}</span> : <PermissionChecks name={user.name} selected={user.permissions} onChange={(permissions) => onChange({ permissions })} />}
      </td>
      <td>
        <Switch label={`${user.name}: ${t('settings.accounts.viewOnly')}`} checked={user.view_only} onChange={(view_only) => onChange({ view_only })} />
      </td>
      <td>
        <Switch label={`${user.name}: ${t('settings.accounts.disabled')}`} checked={user.disabled} onChange={(disabled) => onChange({ disabled })} />
      </td>
      <td>
        <input
          className={s.input}
          style={{ width: 110 }}
          type="password"
          autoComplete="new-password"
          placeholder={t('settings.accounts.newPassword')}
          aria-label={`${user.name}: ${t('settings.accounts.newPassword')}`}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <Button
          aria-label={`${user.name}: ${t('settings.accounts.resetPassword')}`}
          disabled={password.length < MIN_PASSWORD}
          onClick={() => {
            onChange({ password });
            setPassword('');
          }}
        >
          {t('settings.accounts.resetPassword')}
        </Button>
      </td>
      <td>
        <Button variant="danger" aria-label={t('settings.accounts.deleteNamed', { name: user.name })} onClick={onDelete}>
          {t('common.delete')}
        </Button>
      </td>
    </tr>
  );
}

function UserList() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const users = useQuery({ queryKey: USERS_KEY, queryFn: async () => (await unwrap(api.GET('/api/users'))).users });
  const { error, run } = useRun();
  const [deleting, setDeleting] = useState<string | null>(null);
  const [name, setName] = useState('');
  const [password, setPassword] = useState('');
  const [role, setRole] = useState<Role>('user');
  const [permissions, setPermissions] = useState<Permission[]>(['console']);
  const [viewOnly, setViewOnly] = useState(false);
  const refresh = () => qc.invalidateQueries({ queryKey: USERS_KEY });

  const create = (e: FormEvent) => {
    e.preventDefault();
    void run(async () => {
      await unwrap(api.POST('/api/users', { body: { name: name.trim(), password, role, permissions: role === 'admin' ? undefined : permissions, view_only: viewOnly } }));
      setName('');
      setPassword('');
      await refresh();
    });
  };

  return (
    <>
      <Group title={t('settings.accounts.users')}>
        <table className={s.table}>
          <thead>
            <tr>
              <th>{t('settings.accounts.name')}</th>
              <th>{t('settings.accounts.role')}</th>
              <th>{t('settings.accounts.permissions')}</th>
              <th>{t('settings.accounts.viewOnly')}</th>
              <th>{t('settings.accounts.disabled')}</th>
              <th>{t('settings.accounts.password')}</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {users.data?.map((u) => (
              <UserRow
                key={u.name}
                user={u}
                onChange={(part) =>
                  void run(async () => {
                    await unwrap(api.PATCH('/api/users/{name}', { params: { path: { name: u.name } }, body: part as never }));
                    await refresh();
                  })
                }
                onDelete={() => setDeleting(u.name)}
              />
            ))}
          </tbody>
        </table>
      </Group>
      <form onSubmit={create}>
        <Group title={t('settings.accounts.add')}>
          <Row label={t('settings.accounts.name')}>
            <input className={s.input} aria-label={t('settings.accounts.newName')} value={name} onChange={(e) => setName(e.target.value)} required pattern="[A-Za-z0-9._-]{1,32}" />
          </Row>
          <Row label={t('settings.accounts.password')}>
            <input className={s.input} type="password" autoComplete="new-password" aria-label={t('settings.accounts.newUserPassword')} value={password} onChange={(e) => setPassword(e.target.value)} required minLength={MIN_PASSWORD} />
          </Row>
          <Row label={t('settings.accounts.role')}>
            <Select<Role>
              label={t('settings.accounts.newRole')}
              value={role}
              options={[
                { value: 'user', label: t('settings.accounts.roleUser') },
                { value: 'admin', label: t('settings.accounts.roleAdmin') },
              ]}
              onChange={setRole}
            />
          </Row>
          {role === 'user' && (
            <Row label={t('settings.accounts.permissions')}>
              <PermissionChecks name={t('settings.accounts.add')} selected={permissions} onChange={setPermissions} />
            </Row>
          )}
          <Row label={t('settings.accounts.viewOnly')}>
            <Switch label={`${t('settings.accounts.add')}: ${t('settings.accounts.viewOnly')}`} checked={viewOnly} onChange={setViewOnly} />
          </Row>
          <div className={s.actions}>
            <Button type="submit" variant="primary">
              {t('settings.accounts.create')}
            </Button>
          </div>
        </Group>
      </form>
      <Alert message={error} />
      {deleting && (
        <ConfirmDialog
          title={t('settings.accounts.deleteTitle', { name: deleting })}
          message={t('settings.accounts.deleteMessage')}
          confirmLabel={t('common.delete')}
          danger
          onCancel={() => setDeleting(null)}
          onConfirm={() => {
            const target = deleting;
            setDeleting(null);
            void run(async () => {
              await unwrap(api.DELETE('/api/users/{name}', { params: { path: { name: target } } }));
              await refresh();
            });
          }}
        />
      )}
    </>
  );
}

/** Own password for everyone; the account list only for admins. */
export function AccountsPage() {
  const me = useQuery({ queryKey: ME_KEY, queryFn: () => unwrap(api.GET('/api/auth/me')).then((r) => r.user), staleTime: Infinity });
  return (
    <>
      <OwnPassword />
      {me.data?.role === 'admin' && <UserList />}
    </>
  );
}
