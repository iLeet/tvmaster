import { useRef, useState } from 'react';
import { api, assetUrlForPath, uploadAsset, type Asset } from './api';
import { Btn, Modal, Spinner, useToast } from './ui';

/** A picture setting that is chosen by uploading (or picking an earlier upload) instead of typing a server path. */
export function PictureField({ value, onChange }: { value: string | null; onChange: (path: string | null) => void }) {
  const { attempt } = useToast();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState<Asset[] | null>(null);
  const [typed, setTyped] = useState(false);
  const preview = assetUrlForPath(value);

  const upload = async (file: File) => {
    setBusy(true);
    const a = await attempt(() => uploadAsset(file));
    setBusy(false);
    if (a) onChange(a.path);
  };

  return (
    <div className="stack" style={{ gap: 8 }}>
      <div className="row wrap" style={{ alignItems: 'center' }}>
        {preview ? (
          <img src={preview} alt="" style={{ height: 56, maxWidth: 120, objectFit: 'contain', background: 'var(--surface-2, #0002)', borderRadius: 6 }} />
        ) : value ? (
          <span className="mono small dim" style={{ maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis' }}>{value}</span>
        ) : (
          <span className="small dim">No picture chosen</span>
        )}
        <input ref={input} type="file" accept="image/png,image/jpeg,image/gif,image/webp" hidden onChange={(e) => { const f = e.target.files?.[0]; e.target.value = ''; if (f) void upload(f); }} />
        <Btn small onClick={() => input.current?.click()} disabled={busy}>{busy ? 'Uploading…' : value ? 'Replace…' : 'Upload picture…'}</Btn>
        <Btn small onClick={() => attempt(() => api.get<Asset[]>('/assets')).then((l) => l && setPicking(l))}>Earlier uploads…</Btn>
        {value && <Btn small onClick={() => onChange(null)}>Remove</Btn>}
      </div>
      <details open={typed} onToggle={(e) => setTyped((e.target as HTMLDetailsElement).open)}>
        <summary className="small dim" style={{ cursor: 'pointer' }}>Use a file already on the server instead</summary>
        <input className="input mono" style={{ marginTop: 6 }} placeholder="/path/to/picture.png" value={value ?? ''} onChange={(e) => onChange(e.target.value || null)} />
      </details>
      {picking && (
        <Modal title="Earlier uploads" onClose={() => setPicking(null)}>
          {picking.length === 0 ? <p className="dim">Nothing uploaded yet.</p> : (
            <div className="row wrap" style={{ gap: 12 }}>
              {picking.map((a) => (
                <button key={a.id} type="button" className="btn" style={{ padding: 6, display: 'grid', gap: 4, justifyItems: 'center' }} onClick={() => { onChange(a.path); setPicking(null); }}>
                  <img src={a.url} alt="" style={{ height: 72, maxWidth: 140, objectFit: 'contain' }} />
                  <span className="small dim">{a.id.replace(/^[0-9a-f]{16}-?/, '').replace(/\.[a-z]+$/, '') || 'picture'}</span>
                </button>
              ))}
            </div>
          )}
        </Modal>
      )}
      {busy && <Spinner />}
    </div>
  );
}
