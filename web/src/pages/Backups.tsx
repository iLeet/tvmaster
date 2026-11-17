import { useRef, useState } from 'react';
import { api, errorMessage } from '../api';
import { formatBytes } from '../lib';
import { useQuery } from '../live';
import type { BackupList, BackupRestored } from '../types';
import { Badge, Btn, Card, ConfirmBtn, Field, Spinner, Toggle, useToast } from '../ui';

const when = (ms: number) => new Date(ms).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });

/** Back up and restore the station: channels, lineups, libraries, show names, episode positions, settings and pictures. */
export function Backups() {
  const list = useQuery(() => api.get<BackupList>('/backups'));
  const { say, attempt } = useToast();
  const [busy, setBusy] = useState(false);
  const [label, setLabel] = useState('');
  const file = useRef<HTMLInputElement>(null);
  const d = list.data;

  const run = async (what: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await what();
      say(done);
      list.reload();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };
  const restore = (name: string) =>
    run(async () => {
      const r = await api.post<BackupRestored>(`/backups/${encodeURIComponent(name)}/restore`);
      say(`Restored ${r.channels} channels and ${r.libraries} libraries${r.missing_folders.length ? `. ${r.missing_folders.length} library folder${r.missing_folders.length === 1 ? ' is' : 's are'} not on this machine` : ''}. The station as it was is saved as ${r.safety_copy}.`);
    }, 'Restored');
  const upload = async (f: File) => {
    setBusy(true);
    try {
      const res = await fetch(`/api/v1/backups/upload?name=${encodeURIComponent(f.name)}`, { method: 'POST', body: f, credentials: 'same-origin' });
      const j = (await res.json().catch(() => ({}))) as { error?: string; name?: string };
      if (!res.ok) throw new Error(j.error ?? `${res.status} ${res.statusText}`);
      say('Backup added. Restore it from the list below.');
      list.reload();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="stack">
      <h2>Backup &amp; restore</h2>
      <p className="small dim">
        Saves your channels, lineups, libraries (their folders and what was found in them), show names, episode positions, settings and pictures. Your video files are not in it,
        and neither is this server's access token. Restoring first saves what is there now, so you can go back.
      </p>
      {!d ? <Spinner /> : (
        <>
          <div className="row wrap" style={{ alignItems: 'flex-end' }}>
            <Field label="Name (optional)"><input className="input" style={{ width: 200 }} value={label} maxLength={30} placeholder="before the new lineup" onChange={(e) => setLabel(e.target.value)} /></Field>
            <Btn kind="primary" disabled={busy} onClick={() => run(async () => { await api.post('/backups', { label }); setLabel(''); }, 'Backed up')}>{busy ? <Spinner /> : 'Back up now'}</Btn>
            <label className="btn"><input ref={file} type="file" hidden accept=".gz,.tgz,application/gzip" onChange={(e) => { const f = e.target.files?.[0]; if (f) upload(f); e.target.value = ''; }} />Add a backup file…</label>
          </div>
          <div className="row wrap">
            <Toggle checked={d.auto} onChange={(auto) => attempt(() => api.put('/backups/settings', { auto }), auto ? 'Nightly backups on' : 'Nightly backups off').then(list.reload)} label={<span>Back up every night <span className="dim small">the newest {d.keep} are kept</span></span>} />
            {d.auto && (
              <label className="row small" style={{ gap: 6 }}>Keep
                <input className="input num" type="number" min={1} max={60} value={d.keep} onChange={(e) => attempt(() => api.put('/backups/settings', { keep: Number(e.target.value) })).then(list.reload)} style={{ width: 70 }} />
              </label>
            )}
          </div>
          {d.backups.length === 0 ? <p className="small dim">No backups yet.</p> : (
            <div className="list">
              {d.backups.map((b) => (
                <div key={b.name} className="row wrap">
                  <div className="grow">
                    <div>{when(b.created_ms)} {b.auto && <Badge>nightly</Badge>} {b.label && <Badge tone="accent">{b.label}</Badge>}</div>
                    <div className="small dim">{b.counts.channels} channels · {b.counts.libraries} libraries · {b.counts.files} files · {formatBytes(b.size)} · v{b.version}</div>
                  </div>
                  <a className="btn small" href={`/api/v1/backups/${encodeURIComponent(b.name)}`} download={b.name}>Download</a>
                  <ConfirmBtn small label="Restore" confirm={`Put the station back as it was on ${when(b.created_ms)}? Channels stop for a moment and every schedule is planned again. What is there now is saved first.`} onConfirm={() => restore(b.name)} />
                  <ConfirmBtn small kind="danger" icon="trash" label="" aria-label="Delete backup" confirm="Delete this backup?" onConfirm={() => run(() => api.del(`/backups/${encodeURIComponent(b.name)}`), 'Deleted')} />
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </Card>
  );
}
