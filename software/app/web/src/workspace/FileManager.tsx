import { useCallback, useRef, useState, type FormEvent, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, unwrap } from '../api/client';
import { emitLoginRequired } from '../api/authEvents';
import type { components } from '../api/schema';
import { fetchMe, ME_KEY } from '../session/useSession';
import { errorText } from '../console/menus/hooks';
import { bytes } from '../settings/fields';
import { ConfirmDialog, toast } from '../ui';
import { childPath, refreshFiles, useFileTransfers } from './fileTransfers';
import { FileActionMenu } from './FileActionMenu';
import s from './FileManager.module.css';

type Root = components['schemas']['Root'];
type Entry = components['schemas']['FileEntry'];
interface OpenMenu { entry: Entry; x: number; y: number; opener: HTMLElement }
type Confirm = { action: 'delete' | 'mount'; entry: Entry };
const shellRoots: Record<Root, string> = { sd: '/mnt/sd', share: '/mnt/share', system: '' };

export function FileManager({ onOpenTerminal }: { onOpenTerminal?: (path: string) => void }) {
  const { t, i18n } = useTranslation();
  const client = useQueryClient();
  const [root, setRoot] = useState<Root>('sd');
  const [path, setPath] = useState('/');
  const [context, setContext] = useState<OpenMenu | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [editor, setEditor] = useState<{ from?: string } | null>(null);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const pane = useRef<HTMLElement>(null);
  const chooser = useRef<HTMLInputElement>(null);
  const downloadFrame = useRef<HTMLIFrameElement>(null);
  const transfers = useFileTransfers();
  const user = useQuery({ queryKey: ME_KEY, queryFn: fetchMe, staleTime: Infinity }).data;
  const files = useQuery({ queryKey: ['files', root, path], queryFn: () => unwrap(api.GET('/api/files', { params: { query: { root, path } } })) });
  const storage = useQuery({ queryKey: ['storage'], queryFn: () => unwrap(api.GET('/api/storage')) });
  const volume = storage.data?.find(v => v.root === root);
  const roots: Root[] = user?.role === 'admin' ? ['sd', 'share', 'system'] : ['sd', 'share'];
  const rootName = (r: Root) => t(r === 'system' ? 'files.system' : 'settings.storage.root.' + r);
  const parts = path.split('/').filter(Boolean);
  const closeMenu = useCallback(() => setContext(null), []);
  const fmtTime = new Intl.DateTimeFormat(i18n.language, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });

  function navigate(nextRoot: Root, nextPath: string) {
    setRoot(nextRoot); setPath(nextPath);
    setContext(null); setEditor(null); setConfirm(null);
  }
  function showMenu(e: MouseEvent<HTMLElement>, entry: Entry, pointer: boolean) {
    e.preventDefault();
    const opener = e.currentTarget.querySelector<HTMLButtonElement>('button') ?? e.currentTarget;
    const row = opener.getBoundingClientRect();
    const rect = pane.current!.getBoundingClientRect();
    setContext({ entry, opener, x: (pointer ? e.clientX : row.left) - rect.left, y: (pointer ? e.clientY : row.bottom) - rect.top });
  }
  async function change(operation: Promise<unknown>) {
    setBusy(true);
    try {
      await operation;
      refreshFiles(client, root);
      setEditor(null);
    } catch (error) { toast(errorText(t, error), 'error'); }
    finally { setBusy(false); }
  }
  function edit(from?: string) {
    setEditor({ from }); setName(from ?? ''); setContext(null);
  }
  function save(e: FormEvent) {
    e.preventDefault();
    if (!editor || busy) return;
    if (!name.trim() || name === '.' || name === '..' || /[\/\x00]/.test(name)) {
      toast(t('error.bad_path'), 'error'); return;
    }
    const to = childPath(path, name);
    if (editor.from) {
      void change(unwrap(api.POST('/api/files/rename', { body: { root, path: childPath(path, editor.from), to } })));
    } else {
      void change(unwrap(api.POST('/api/files/mkdir', { body: { root, path: to } })));
    }
  }
  function confirmAction() {
    if (!confirm) return;
    const target = childPath(path, confirm.entry.name);
    setConfirm(null);
    if (confirm.action === 'delete') {
      void change(unwrap(api.POST('/api/files/delete', { body: { root, path: target } })));
    } else {
      void change(unwrap(api.POST('/api/media/mount', { body: { root: 'sd', path: target, mode: 'cdrom' } }))
        .then(media => { client.setQueryData(['media', 'mounted'], media); }));
    }
  }
  // Content-Disposition hands successful files directly to the browser's download
  // manager. Only the contract's JSON error responses navigate the frame.
  function download() {
    downloadFrame.current!.src = '/api/files/download?' + new URLSearchParams({ root, path: selected });
    closeMenu();
  }
  function readDownloadError() {
    const frame = downloadFrame.current!;
    if (!frame.hasAttribute('src')) return;
    try {
      const doc = frame.contentDocument;
      if (doc?.URL === 'about:blank') return;
      const text = doc?.querySelector('pre')?.textContent ?? doc?.body?.textContent;
      const error: components['schemas']['Error'] = JSON.parse(text ?? '');
      if (error.error === 'unauthenticated') emitLoginRequired();
      else toast(t('error.' + error.error, { ...error.detail, defaultValue: t('error.unknown') }), 'error');
    } catch {
      toast(t('error.unknown'), 'error');
    }
  }
  const selectAction = (action: Confirm['action']) => {
    if (context) setConfirm({ action, entry: context.entry });
    closeMenu();
  };
  const entry = context?.entry;
  const selected = entry ? childPath(path, entry.name) : path;
  const shellDirectory = shellRoots[root] + (entry?.type === 'dir' ? selected : path);
  const mayOpenTerminal = onOpenTerminal && user?.permissions.includes('terminal') && !/[\x00-\x1f\x7f]/.test(shellDirectory);
  const currentError = files.error ?? storage.error;

  return <section className={s.pane} ref={pane} aria-label={t('files.title')}
    onDragOver={e => { if (e.dataTransfer.types.includes('Files')) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; } }}
    onDrop={e => { e.preventDefault(); transfers.enqueue(e.dataTransfer.files, { root, path }); }}>
    <iframe ref={downloadFrame} hidden title={t('files.download')} sandbox="allow-same-origin allow-downloads" onLoad={readDownloadError}/>
    <div className={s.roots}>{roots.map(r =>
      <button key={r} aria-pressed={r === root} onClick={() => navigate(r, '/')}>{rootName(r)}</button>)}</div>
    <div className={s.toolbar}>
      <nav className={s.crumbs} aria-label={t('files.path')}>
        <button onClick={() => navigate(root, '/')}>{rootName(root)}</button>
        {parts.map((part, i) => <span key={i}>› <button onClick={() => navigate(root, '/' + parts.slice(0, i + 1).join('/'))}>{part}</button></span>)}
      </nav>
      <input type="file" multiple ref={chooser} hidden aria-label={t('files.choose')} onChange={e => {
        if (e.target.files) transfers.enqueue(e.target.files, { root, path });
        e.target.value = '';
      }}/>
      <button onClick={() => chooser.current?.click()}>{t('files.upload')}</button>
      <button onClick={() => edit()} disabled={busy}>{t('files.mkdir')}</button>
      <button title={t('files.refresh')} aria-label={t('files.refresh')} onClick={() => refreshFiles(client, root)}>↻</button>
    </div>
    {editor && <form className={s.editor} onSubmit={save}>
      <input autoFocus aria-label={t('files.name')} value={name} placeholder={t('files.name')} onChange={e => setName(e.target.value)}
        onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); setEditor(null); } }} />
      <button type="submit" disabled={busy}>{t(editor.from ? 'files.rename' : 'files.create')}</button>
      <button type="button" disabled={busy} onClick={() => setEditor(null)}>{t('common.cancel')}</button>
    </form>}
    {currentError && <p role="alert" className={s.error}>{errorText(t, currentError)}</p>}
    <div className={s.list}>
      <table><thead><tr><th>{t('files.name')}</th><th>{t('files.size')}</th><th>{t('files.modified')}</th><th /></tr></thead>
        <tbody>{files.data?.entries.map(item => <tr key={item.name} data-selected={entry?.name === item.name || undefined}
          onContextMenu={e => showMenu(e, item, true)}>
          <td><button className={s.filename} aria-label={item.name} onClick={e => item.type === 'dir' ? navigate(root, childPath(path, item.name)) : showMenu(e, item, false)}>
            <span aria-hidden className={item.type === 'dir' ? s.folder : s.file}>{item.type === 'dir' ? '▸' : /\.iso$/i.test(item.name) ? '◉' : '▤'}</span>
            <span>{item.name}</span></button>{item.mounted && <small className={s.mounted}>{t('media.mounted')}</small>}</td>
          <td>{item.type === 'dir' ? '—' : bytes(item.size)}</td><td>{fmtTime.format(item.mtime)}</td>
          <td><button aria-label={t('files.actions', { name: item.name })} aria-haspopup="menu" onClick={e => showMenu(e, item, false)}>⋯</button></td>
        </tr>)}</tbody>
      </table>
      {!currentError && !files.data?.entries.length && <p className={s.hint}>{t(files.isPending ? 'files.loading' : 'files.empty')}</p>}
    </div>
    {transfers.jobs.length > 0 && <ul className={s.transfers}>{transfers.jobs.map(j => {
      const ended = j.state === 'failed' || j.state === 'cancelled';
      return <li key={j.id}><span title={j.target.path}>{j.file.name}</span>
        <span role="status">{t('files.' + j.state, { progress: j.progress })}</span>
        {ended && <button onClick={() => transfers.retry(j.id)}>{t('files.retry')}</button>}
        <button onClick={() => ended ? transfers.dismiss(j.id) : transfers.cancel(j.id)}>{t(ended ? 'common.close' : 'common.cancel')}</button>
      </li>;
    })}</ul>}
    <div className={s.drop}>{t('files.drop')}</div>
    <footer className={s.footer}><span>{t('files.count', { count: files.data?.entries.length ?? 0 })}</span>
      <span>{volume && t('settings.storage.usage', { free: bytes(volume.free), total: bytes(volume.total) })}</span></footer>
    {context && entry && <FileActionMenu {...context} label={entry.name} onClose={closeMenu}>
      {entry.type === 'file' && <button role="menuitem" onClick={download}>{t('files.download')}</button>}
      <button role="menuitem" onClick={() => edit(entry.name)}>{t('files.rename')}</button>
      {entry.type === 'file' && root === 'sd' && /\.iso$/i.test(entry.name) && !entry.mounted && user?.permissions.includes('media') &&
        <button role="menuitem" onClick={() => selectAction('mount')}>{t('files.mount')}</button>}
      {mayOpenTerminal && <button role="menuitem" onClick={() => { closeMenu(); onOpenTerminal(shellDirectory); }}>{t('files.openTerminal')}</button>}
      <button role="menuitem" className={s.danger} onClick={() => selectAction('delete')}>{t('common.delete')}</button>
    </FileActionMenu>}
    {confirm && <ConfirmDialog title={t(confirm.action === 'delete' ? 'common.delete' : 'files.mount')}
      message={t(confirm.action === 'delete' ? 'files.deleteMessage' : 'files.mountMessage', { name: confirm.entry.name })}
      confirmLabel={t(confirm.action === 'delete' ? 'common.delete' : 'files.mount')} danger={confirm.action === 'delete'}
      onCancel={() => setConfirm(null)} onConfirm={confirmAction}/>}
  </section>;
}
