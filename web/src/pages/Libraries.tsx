import { useEffect, useState } from 'react';
import { api, errorMessage, uploadLibraryFile } from '../api';
import { formatBytes, uploadPath } from '../lib';
import { useQuery } from '../live';
import type { FsList, Library, LibraryKind, System } from '../types';
import { Badge, Btn, Card, ConfirmBtn, Empty, Icon, Modal, Progress, Segmented, Spinner, Toggle, useToast } from '../ui';

const KINDS: { value: LibraryKind; label: string; help: string }[] = [
  { value: 'shows', label: 'TV shows', help: 'Folders of episodes (Show/Season 1/Show - S01E02 - Title.mkv). These feed your channels.' },
  { value: 'commercials', label: 'Commercials', help: 'Retro ads. Put the year in a folder or file name (1987/…) to filter ads by era.' },
  { value: 'bumpers', label: 'Bumper clips', help: 'Your own station IDs and stings, used to open commercial breaks.' },
  { value: 'music', label: 'Music', help: 'Background music for the guide channel.' },
];

export const kindLabel = (k: string) => KINDS.find((x) => x.value === k)?.label ?? k;

function FolderPicker({ start, onPick, onClose }: { start?: string; onPick: (path: string) => void; onClose: () => void }) {
  const [path, setPath] = useState(start ?? '');
  const q = useQuery(() => api.get<FsList>(`/fs/list?path=${encodeURIComponent(path)}`), [path]);
  const data = q.data;
  return (
    <Modal
      title="Choose a folder on the server"
      onClose={onClose}
      footer={
        <>
          <Btn onClick={onClose}>Cancel</Btn>
          <Btn kind="primary" disabled={!data} onClick={() => data && onPick(data.path)}>Use this folder</Btn>
        </>
      }
    >
      <div className="stack">
        <div className="row">
          <Btn small icon="up" disabled={!data?.parent} onClick={() => data?.parent && setPath(data.parent)}>Up</Btn>
          <div className="crumbs mono grow">{data?.path ?? '…'}</div>
        </div>
        {q.error && <div className="err-banner">{q.error}</div>}
        <div className="folders">
          {q.loading && !data ? <div className="empty"><Spinner /></div> : (data?.dirs.length ?? 0) === 0 ? (
            <div className="empty small">No subfolders here.</div>
          ) : (
            data?.dirs.map((d) => (
              <div key={d.path} className="folder" onClick={() => setPath(d.path)}>
                <Icon name="folder" /> {d.name}
              </div>
            ))
          )}
        </div>
        <p className="small dim">Folders are on the machine running TVMaster, not on this device.</p>
      </div>
    </Modal>
  );
}

const ACCEPT: Record<string, string> = {
  shows: 'video/*,.mkv,.avi,.m2ts,.ts,.srt,.ass,.ssa,.vtt',
  commercials: 'video/*,.mkv,.avi,.m2ts,.ts',
  bumpers: 'video/*,.mkv,.avi,.m2ts,.ts',
  music: 'audio/*',
};

interface QueueItem { file: File; path: string; progress: number; state: 'waiting' | 'sending' | 'done' | 'failed' | 'exists'; message?: string }

/** Upload files (or a whole folder) from this device into a library's folder on the server. */
function UploadFiles({ library, onClose, onDone }: { library: Library; onClose: () => void; onDone: () => void }) {
  const [sub, setSub] = useState('');
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [running, setRunning] = useState(false);
  const { say } = useToast();
  const patch = (i: number, p: Partial<QueueItem>) => setQueue((q) => q.map((x, j) => (j === i ? { ...x, ...p } : x)));
  const add = (files: FileList | null) => {
    if (!files) return;
    setQueue((q) => [...q, ...Array.from(files).map((file) => ({ file, path: uploadPath(sub, file), progress: 0, state: 'waiting' as const }))]);
  };
  const total = queue.reduce((n, x) => n + x.file.size, 0);

  const start = async (overwrite = false) => {
    setRunning(true);
    for (let i = 0; i < queue.length; i++) {
      const it = queue[i];
      if (it.state === 'done' || (it.state === 'exists' && !overwrite)) continue;
      patch(i, { state: 'sending', progress: 0, message: undefined });
      try {
        await uploadLibraryFile(library.id, it.path, it.file, (f) => patch(i, { progress: f }), overwrite && it.state === 'exists');
        patch(i, { state: 'done', progress: 1 });
      } catch (e) {
        const m = errorMessage(e);
        patch(i, { state: /already there/.test(m) ? 'exists' : 'failed', message: m });
      }
    }
    setRunning(false);
    onDone();
    say('Upload finished');
  };

  const pending = queue.some((x) => x.state === 'waiting' || x.state === 'failed');
  return (
    <Modal
      title={`Upload to ${library.path.split(/[\\/]/).filter(Boolean).pop() ?? library.path}`}
      onClose={running ? () => undefined : onClose}
      footer={
        <>
          <Btn disabled={running} onClick={onClose}>Close</Btn>
          {queue.some((x) => x.state === 'exists') && <Btn disabled={running} onClick={() => start(true)}>Replace the ones already there</Btn>}
          <Btn kind="primary" disabled={running || !pending} onClick={() => start()}>{running ? <Spinner /> : `Upload ${queue.length || ''} file${queue.length === 1 ? '' : 's'}${queue.length ? ` (${formatBytes(total)})` : ''}`}</Btn>
        </>
      }
    >
      <div className="stack">
        <p className="small dim">The files are copied into this library's folder on the server, then added to the schedule. {library.kind === 'shows' ? 'Pick a whole folder to keep its Show/Season layout.' : ''}</p>
        <div className="row wrap" style={{ alignItems: 'flex-end' }}>
          <label className="btn"><input type="file" multiple hidden accept={ACCEPT[library.kind]} onChange={(e) => { add(e.target.files); e.target.value = ''; }} />Choose files…</label>
          <label className="btn"><input type="file" hidden multiple {...({ webkitdirectory: '' } as object)} onChange={(e) => { add(e.target.files); e.target.value = ''; }} />Choose a folder…</label>
          <input className="input" style={{ width: 220 }} placeholder="Sub-folder (optional)" value={sub} onChange={(e) => setSub(e.target.value)} aria-label="Sub-folder" />
        </div>
        {queue.length > 0 && (
          <div className="stack" style={{ gap: 6, maxHeight: 280, overflow: 'auto' }}>
            {queue.map((x, i) => (
              <div key={i} className="stack" style={{ gap: 2 }}>
                <div className="row small"><span className="grow ellipsis mono" title={x.path}>{x.path}</span><span className="dim">{formatBytes(x.file.size)}</span>
                  <Badge tone={x.state === 'done' ? 'ok' : x.state === 'failed' ? 'warn' : ''}>{x.state === 'exists' ? 'already there' : x.state}</Badge></div>
                {x.state === 'sending' && <Progress value={Math.round(x.progress * 100)} max={100} />}
                {x.message && x.state !== 'exists' && <div className="small" style={{ color: 'var(--warn)' }}>{x.message}</div>}
              </div>
            ))}
          </div>
        )}
      </div>
    </Modal>
  );
}

function AddLibrary({ onClose, onAdded }: { onClose: () => void; onAdded: () => void }) {
  const [kind, setKind] = useState<LibraryKind>('shows');
  const [path, setPath] = useState('');
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [create, setCreate] = useState(false);
  const sys = useQuery(() => api.get<System>('/system'));
  const { say } = useToast();

  const add = async () => {
    setBusy(true);
    try {
      const lib = await api.post<Library>('/libraries', { path, kind, create });
      onClose();
      say('Library added; scanning…');
      const r = await api.post<{ found: number; indexed: number; skipped: number }>(`/libraries/${lib.id}/scan`);
      say(`Found ${r.found} files, indexed ${r.indexed}${r.skipped ? `, skipped ${r.skipped}` : ''}`);
      onAdded();
    } catch (e) {
      say(errorMessage(e), true);
      setBusy(false);
    }
  };

  return (
    <>
      <Modal
        title="Add a library"
        onClose={onClose}
        footer={
          <>
            <Btn onClick={onClose}>Cancel</Btn>
            <Btn kind="primary" disabled={!path || busy} onClick={add}>{busy ? <Spinner /> : 'Add and scan'}</Btn>
          </>
        }
      >
        <div className="stack">
          <Segmented value={kind} options={KINDS} onChange={setKind} />
          <p className="small dim">{KINDS.find((k) => k.value === kind)?.help}</p>
          <div className="row">
            <input className="input mono" placeholder="/path/on/the/server" value={path} onChange={(e) => setPath(e.target.value)} />
            <Btn icon="folder" onClick={() => setPicking(true)}>Browse…</Btn>
          </div>
          {sys.data?.data_dir && (
            <div className="row wrap small">
              <Btn small onClick={() => { setPath(`${sys.data!.data_dir}/media/${kind}`.replace(/\\/g, '/')); setCreate(true); }}>Use a new folder in the server's data folder</Btn>
              <span className="dim">for files you will upload from here</span>
            </div>
          )}
          <Toggle checked={create} onChange={setCreate} label={<span>Make the folder if it does not exist <span className="dim small">(you can then upload files into it)</span></span>} />
        </div>
      </Modal>
      {picking && <FolderPicker start={path || undefined} onClose={() => setPicking(false)} onPick={(p) => { setPath(p); setPicking(false); }} />}
    </>
  );
}

export function Libraries() {
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const [adding, setAdding] = useState(false);
  const [uploading, setUploading] = useState<Library | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const { say, attempt } = useToast();

  // Poll while break detection is running so the progress bar moves.
  const analysing = (libs.data ?? []).some((l) => l.analysis?.running);
  useEffect(() => {
    if (!analysing) return;
    const t = setInterval(libs.reload, 2000);
    return () => clearInterval(t);
  }, [analysing, libs.reload]);

  const scan = async (l: Library) => {
    setBusyId(l.id);
    const r = await attempt(() => api.post<{ found: number; indexed: number; skipped: number; analysing: number }>(`/libraries/${l.id}/scan`));
    setBusyId(null);
    if (r) say(`Found ${r.found} files, indexed ${r.indexed}${r.skipped ? `, skipped ${r.skipped}` : ''}${r.analysing ? `, analysing ${r.analysing} for ad breaks` : ''}`);
    libs.reload();
  };

  return (
    <div className="page">
      <div className="page-head">
        <h1>Libraries</h1>
        <span className="sub">Folders of shows, commercials, bumpers and music</span>
        <Btn kind="primary" icon="plus" className="right" onClick={() => setAdding(true)}>Add library</Btn>
      </div>
      {libs.error && <div className="err-banner">{libs.error}</div>}
      <Card>
        {libs.loading && !libs.data ? <div className="empty"><Spinner /></div> : (libs.data ?? []).length === 0 ? (
          <Empty>No libraries yet. Add the folder where your TV collection lives.</Empty>
        ) : (
          <div className="list">
            {libs.data!.map((l) => (
              <div key={l.id} className="stack" style={{ gap: 8, opacity: l.enabled ? 1 : 0.55 }}>
                <div className="row wrap">
                  <Icon name="folder" />
                  <div className="grow">
                    <div className="mono ellipsis" title={l.path}>{l.path}</div>
                    <div className="small dim">{l.items.toLocaleString()} files</div>
                  </div>
                  <Badge tone={l.kind === 'shows' ? 'accent' : ''}>{kindLabel(l.kind)}</Badge>
                  <Toggle checked={l.enabled} onChange={(v) => attempt(() => api.patch(`/libraries/${l.id}`, { enabled: v }), v ? 'Library enabled' : 'Library disabled').then(libs.reload)} />
                  <Btn small icon="plus" onClick={() => setUploading(l)}>Upload</Btn>
                  <Btn small icon="scan" disabled={busyId === l.id} onClick={() => scan(l)}>{busyId === l.id ? <Spinner /> : 'Scan'}</Btn>
                  {l.kind === 'shows' && (
                    <Btn small icon="refresh" title="Find ad-break points (black frames) for files that have none" onClick={() => attempt(() => api.post<{ queued: number }>(`/libraries/${l.id}/analyze`)).then((r) => { if (r) say(r.queued ? `Analysing ${r.queued} files` : 'Nothing to analyse'); libs.reload(); })}>Analyse</Btn>
                  )}
                  <ConfirmBtn small kind="danger" icon="trash" label="" confirm={`Remove this library from TVMaster? Your files on disk are not touched. Channels using it will be rescheduled.`} onConfirm={() => attempt(() => api.del(`/libraries/${l.id}`), 'Library removed').then(libs.reload)} aria-label="Remove library" />
                </div>
                {l.analysis && (
                  <div className="row small dim">
                    <div className="grow"><Progress value={l.analysis.done} max={l.analysis.total} /></div>
                    <span>{l.analysis.running ? `Finding ad breaks ${l.analysis.done}/${l.analysis.total}` : `Ad breaks found for ${l.analysis.total} files`}</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </Card>
      {adding && <AddLibrary onClose={() => setAdding(false)} onAdded={libs.reload} />}
      {uploading && <UploadFiles library={uploading} onClose={() => setUploading(null)} onDone={libs.reload} />}
    </div>
  );
}
