import { useEffect, useState } from 'react';
import { api, errorMessage } from '../api';
import { useQuery } from '../live';
import type { FsList, Library, LibraryKind } from '../types';
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

function AddLibrary({ onClose, onAdded }: { onClose: () => void; onAdded: () => void }) {
  const [kind, setKind] = useState<LibraryKind>('shows');
  const [path, setPath] = useState('');
  const [picking, setPicking] = useState(false);
  const [busy, setBusy] = useState(false);
  const { say } = useToast();

  const add = async () => {
    setBusy(true);
    try {
      const lib = await api.post<Library>('/libraries', { path, kind });
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
        </div>
      </Modal>
      {picking && <FolderPicker start={path || undefined} onClose={() => setPicking(false)} onPick={(p) => { setPath(p); setPicking(false); }} />}
    </>
  );
}

export function Libraries() {
  const libs = useQuery(() => api.get<Library[]>('/libraries'));
  const [adding, setAdding] = useState(false);
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
    </div>
  );
}
