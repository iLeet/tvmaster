import { useState } from 'react';
import { api, errorMessage } from '../api';
import { PictureField } from '../PictureField';
import type { ShowInfo, ShowSummary } from '../types';
import { Btn, Field, Modal, Spinner, useToast } from '../ui';

/** What the station calls a show, which folders it is made of, and a description, rating, tags and poster for it. */
export function ShowDetails({ show, onClose, onChanged }: { show: ShowSummary; onClose: () => void; onChanged: () => void }) {
  const { say } = useToast();
  const [title, setTitle] = useState(show.name);
  const [info, setInfo] = useState<ShowInfo>(show.info ?? { description: '', rating: '', tags: [], poster: null });
  const [tags, setTags] = useState((show.info?.tags ?? []).join(', '));
  const [busy, setBusy] = useState(false);

  const run = async (what: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await what();
      say(done);
      onChanged();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    run(async () => {
      const t = title.trim();
      if (t && t !== show.name) await api.post('/shows/merge', { shows: [show.name], title: t });
      await api.put('/shows/info', { show: t || show.name, info: { ...info, tags: tags.split(',').map((x) => x.trim()).filter(Boolean) } });
    }, 'Saved');
  const season = (raw: string, value: string) =>
    run(() => api.put('/shows/folder', { raw, title: show.name, season: value === '' ? null : Number(value) }), 'Season saved');
  const unnumbered = show.folders.length > 1 && show.folders.some((f) => f.season === null);

  return (
    <Modal title={show.name || 'Show'} onClose={onClose} footer={<><Btn onClick={onClose}>Close</Btn><Btn kind="primary" disabled={busy} onClick={save}>{busy ? <Spinner /> : 'Save'}</Btn></>}>
      <div className="stack">
        <Field label="Title on air" hint="What the guide, the schedule and the apps call this show. The folder names stay as they are.">
          <input className="input" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={120} />
        </Field>
        <Field label={`Folders (${show.folders.length})`} hint={show.folders.length > 1 ? 'One show made of several folders. Give each its season so the episodes air in order.' : 'The folder this show was found in.'}>
          <div className="stack" style={{ gap: 6 }}>
            {show.folders.map((f) => (
              <div key={f.raw} className="row small">
                <span className="grow ellipsis" title={f.raw}>{f.raw} <span className="dim">· {f.episodes} episode{f.episodes === 1 ? '' : 's'}</span></span>
                <label className="row small" style={{ gap: 6 }}>Season
                  <input className="input num" type="number" min={0} max={99} defaultValue={f.season ?? ''} onBlur={(e) => e.target.value !== String(f.season ?? '') && season(f.raw, e.target.value)} style={{ width: 70 }} />
                </label>
                {f.custom && <Btn small kind="ghost" onClick={() => run(() => api.post('/shows/folder/reset', { raw: f.raw }), 'Back to the automatic name')}>Reset</Btn>}
              </div>
            ))}
          </div>
          {unnumbered && <p className="small" style={{ color: 'var(--warn)' }}>Some folders have no season, so their episodes will mix with the others by episode number. Give each one a season.</p>}
        </Field>
        <Field label="Description" hint="Shown in IPTV guides and the apps.">
          <textarea className="input" rows={3} maxLength={2000} value={info.description} onChange={(e) => setInfo({ ...info, description: e.target.value })} />
        </Field>
        <div className="row wrap" style={{ alignItems: 'flex-start' }}>
          <Field label="Rating" hint="Day parts use it to keep strong shows out of the morning."><input className="input" style={{ width: 110 }} maxLength={20} list="ratings" value={info.rating} onChange={(e) => setInfo({ ...info, rating: e.target.value })} /><datalist id="ratings">{['TV-Y', 'TV-Y7', 'TV-G', 'TV-PG', 'TV-14', 'TV-MA', 'G', 'PG', 'PG-13', 'R', 'C', 'C8', '14+', '18+'].map((r) => <option key={r} value={r} />)}</datalist></Field>
          <Field label="Tags" hint="Separated by commas: sitcom, 80s"><input className="input" style={{ width: 260 }} value={tags} onChange={(e) => setTags(e.target.value)} /></Field>
        </div>
        <Field label="Poster"><PictureField value={info.poster} onChange={(poster) => setInfo({ ...info, poster })} /></Field>
      </div>
    </Modal>
  );
}
