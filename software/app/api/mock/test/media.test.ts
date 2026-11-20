import { describe, expect, it } from 'vitest';
import { buildServer } from '../src/server.js';
import { check, login } from './helpers.js';

describe('virtual media', () => {
  it('mounts an ISO, marks it in the listing, refuses a second mount and ejects', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const mount = (payload: object) => app.inject({ method: 'POST', url: '/api/media/mount', headers: { cookie }, payload });

    const before = await app.inject({ method: 'GET', url: '/api/media', headers: { cookie } });
    check('getMedia', before);
    expect(before.json()).toEqual({ mounted: null, readonly: true });

    const ok = await mount({ root: 'sd', path: '/images/ubuntu-24.04.iso', mode: 'cdrom' });
    expect(ok.statusCode).toBe(200);
    check('mountMedia', ok);
    const media = await app.inject({ method: 'GET', url: '/api/media', headers: { cookie } });
    expect(media.json()).toEqual({ mounted: { root: 'sd', path: '/images/ubuntu-24.04.iso', mode: 'cdrom' }, readonly: true });
    expect(app.kvm.media).toEqual({ mounted: 'ubuntu-24.04.iso', mode: 'cdrom' });

    const ls = await app.inject({ method: 'GET', url: '/api/files?root=sd&path=%2Fimages', headers: { cookie } });
    const iso = ls.json().entries.find((e: { name: string }) => e.name === 'ubuntu-24.04.iso');
    expect(iso.mounted).toBe(true);

    const again = await mount({ root: 'sd', path: '/images/win11-24h2.iso', mode: 'cdrom' });
    expect(again.statusCode).toBe(409);
    expect(again.json().error).toBe('already_mounted');

    const eject = await app.inject({ method: 'POST', url: '/api/media/eject', headers: { cookie } });
    expect(eject.statusCode).toBe(204);
    check('ejectMedia', eject);
    expect((await app.inject({ method: 'GET', url: '/api/media', headers: { cookie } })).json().mounted).toBeNull();
  });

  it('rejects images it cannot present', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const mount = (payload: object) => app.inject({ method: 'POST', url: '/api/media/mount', headers: { cookie }, payload });
    const txt = await mount({ root: 'sd', path: '/images/notes.txt', mode: 'disk' });
    expect(txt.statusCode).toBe(422);
    expect(txt.json().error).toBe('bad_image');
    check('mountMedia', txt);
    const imgAsCd = await mount({ root: 'sd', path: '/backup/pc-2026-10.img', mode: 'cdrom' });
    expect(imgAsCd.json().error).toBe('bad_image');
    const missing = await mount({ root: 'sd', path: '/images/none.iso', mode: 'cdrom' });
    expect(missing.statusCode).toBe(404);
    const disk = await mount({ root: 'sd', path: '/backup/pc-2026-10.img', mode: 'disk', readonly: false });
    expect(disk.statusCode).toBe(200);
    expect((await app.inject({ method: 'GET', url: '/api/media', headers: { cookie } })).json().readonly).toBe(false);
  });

  it('walks a URL download job through pause, resume and cancel', async () => {
    const app = await buildServer();
    const cookie = await login(app);
    const bad = await app.inject({ method: 'POST', url: '/api/media/download', headers: { cookie }, payload: { url: 'ftp://x/y.iso', path: '/images/y.iso' } });
    expect(bad.statusCode).toBe(400);
    expect(bad.json().error).toBe('bad_url');

    const start = await app.inject({
      method: 'POST', url: '/api/media/download', headers: { cookie },
      payload: { url: 'https://releases.example.org/debian-13.iso', path: '/images/debian-13.iso' },
    });
    expect(start.statusCode).toBe(202);
    check('startDownload', start);
    const id = start.json().job.id;
    expect(start.json().job).toMatchObject({ state: 'running', path: '/images/debian-13.iso' });

    const act = (a: string) => app.inject({ method: 'POST', url: `/api/media/jobs/${id}/${a}`, headers: { cookie } });
    const p = await act('pause');
    check('controlJob', p);
    expect(p.json().state).toBe('paused');
    expect((await act('pause')).statusCode).toBe(409);
    expect((await act('resume')).json().state).toBe('running');
    expect((await act('cancel')).json().state).toBe('canceled');
    expect((await act('resume')).statusCode).toBe(409);

    const jobs = await app.inject({ method: 'GET', url: '/api/media/jobs', headers: { cookie } });
    check('listJobs', jobs);
    expect(jobs.json().jobs[0]).toMatchObject({ id, state: 'canceled' });
    const ls = await app.inject({ method: 'GET', url: '/api/files?root=sd&path=%2Fimages', headers: { cookie } });
    expect(ls.json().entries.map((e: { name: string }) => e.name)).not.toContain('debian-13.iso');
    await app.close();
  });
});
