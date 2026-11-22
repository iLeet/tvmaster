import { useEffect, useRef, useState } from 'react';
import { api } from '../api';
import { useQuery } from '../live';
import { Badge, Btn, Card, ConfirmBtn, Spinner, Toggle, useToast } from '../ui';

export interface Look {
  id: string;
  name: string;
  about: string;
  builtin: boolean;
  theme: { theme: string; style: string; gentle: boolean };
  guide: unknown;
  preview_guide: unknown;
  screens: Record<string, unknown>;
}

/** The part of a look that can be kept in a file and brought back. */
export function lookFile(l: Look): string {
  const { name, about, theme, guide, screens } = l;
  return JSON.stringify({ name, about, theme, guide, screens }, null, 2);
}

/** A picture of the guide in a look. */
function GuideThumb({ guide }: { guide: unknown }) {
  const [url, setUrl] = useState<string | null>(null);
  const last = useRef<string | null>(null);
  useEffect(() => {
    let live = true;
    (async () => {
      try {
        const res = await fetch('/api/v1/guide/preview', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ name: 'Guide', id: 'preview', guide, t: 2 }),
          credentials: 'same-origin',
        });
        if (!res.ok || !live) return;
        const next = URL.createObjectURL(await res.blob());
        if (last.current) URL.revokeObjectURL(last.current);
        last.current = next;
        setUrl(next);
      } catch {
        /* a convenience */
      }
    })();
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(guide)]);
  return url ? <img src={url} alt="" style={{ width: '100%', display: 'block', aspectRatio: '16 / 9', background: '#000', borderRadius: 8 }} /> : <div className="empty" style={{ aspectRatio: '16 / 9' }}><Spinner /></div>;
}

function download(name: string, text: string) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  a.download = `${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}.tvmaster-look.json`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

/** Whole appearances in one click: colours, screens and how the guides are laid out. */
export function Looks({ onApplied }: { onApplied: () => void }) {
  const { attempt, say } = useToast();
  const looks = useQuery(() => api.get<{ looks: Look[] }>('/looks'));
  const [open, setOpen] = useState<string | null>(null);
  const [parts, setParts] = useState({ theme: true, screens: true, guides: true });
  const [name, setName] = useState('');
  const file = useRef<HTMLInputElement>(null);
  const list = looks.data?.looks ?? [];

  const apply = (l: Look) => attempt(async () => {
    await api.post(`/looks/${l.id}/apply`, parts);
    setOpen(null);
    onApplied();
  }, `“${l.name}” is on. Channels on air pick it up now`);

  return (
    <Card className="stack">
      <div>
        <h2>Looks</h2>
        <p className="small dim">A whole appearance in one click: the colours and style, the stand-by and break screens, and how the guides are laid out. Pick one, then change anything you like below, on the Screens page or on a guide.</p>
      </div>
      {!looks.data ? <Spinner /> : (
        <div className="gallery">
          {list.map((l) => (
            <div key={l.id} className={`tile ${open === l.id ? 'on' : ''}`} style={{ cursor: 'default' }}>
              <GuideThumb guide={l.preview_guide} />
              <div className="row between">
                <strong>{l.name}</strong>
                {!l.builtin && <Badge>Mine</Badge>}
              </div>
              <span className="small dim">{l.about}</span>
              <span className="small dim">{l.theme.theme} · {l.theme.style}{l.theme.gentle ? ' · gentle alerts' : ''}{Object.keys(l.screens).length ? ` · ${Object.keys(l.screens).length} screens` : ''}</span>
              {open === l.id ? (
                <div className="stack" style={{ gap: 6 }}>
                  <Toggle checked={parts.theme} onChange={(theme) => setParts({ ...parts, theme })} label="Colours and style" />
                  <Toggle checked={parts.screens} onChange={(screens) => setParts({ ...parts, screens })} label="Stand-by and break screens" />
                  <Toggle checked={parts.guides} onChange={(guides) => setParts({ ...parts, guides })} label="Guide layouts" />
                  <div className="row"><Btn kind="primary" small onClick={() => apply(l)}>Use it</Btn><Btn small onClick={() => setOpen(null)}>Cancel</Btn></div>
                </div>
              ) : (
                <div className="row wrap">
                  <Btn small kind="primary" onClick={() => setOpen(l.id)}>Use this look…</Btn>
                  <Btn small onClick={() => download(l.name, lookFile(l))}>Export</Btn>
                  {!l.builtin && <ConfirmBtn small label="Delete" confirm={`Forget “${l.name}”? The station keeps looking as it does now.`} onConfirm={() => attempt(async () => { await api.del(`/looks/${l.id}`); looks.reload(); }, 'Deleted')} />}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
      <div className="row wrap" style={{ alignItems: 'center' }}>
        <input className="input" style={{ width: 240 }} placeholder="Name for the current look" maxLength={40} value={name} onChange={(e) => setName(e.target.value)} />
        <Btn disabled={!name.trim()} onClick={() => attempt(async () => { await api.post('/looks', { name: name.trim() }); setName(''); looks.reload(); }, 'Saved. It is in the list above')}>Save how the station looks now</Btn>
        <input ref={file} type="file" accept="application/json,.json" hidden onChange={async (e) => {
          const f = e.target.files?.[0];
          e.target.value = '';
          if (!f) return;
          try { await api.post('/looks/import', JSON.parse(await f.text())); looks.reload(); say('Imported. It is in the list above'); } catch { say('That file is not a look', true); }
        }} />
        <Btn onClick={() => file.current?.click()}>Import a look…</Btn>
      </div>
    </Card>
  );
}
