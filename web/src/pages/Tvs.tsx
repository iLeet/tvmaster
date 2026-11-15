import { useState } from 'react';
import { api } from '../api';
import { slugify, validateId } from '../lib';
import { useLiveEvent, useQuery } from '../live';
import { href } from '../router';
import type { ChannelView, Tv } from '../types';
import { Badge, Btn, Card, ConfirmBtn, Empty, Field, Icon, Modal, Spinner, Toggle, useToast } from '../ui';

function TvCard({ tv, channels, onChange }: { tv: Tv; channels: ChannelView[]; onChange: () => void }) {
  const { attempt } = useToast();
  const [volume, setVolume] = useState(tv.volume);
  const send = (path: string, body: unknown) => attempt(() => api.post(`/tvs/${tv.id}/${path}`, body)).then(onChange);
  return (
    <Card className="stack">
      <div className="row">
        <span className={`dot ${tv.online ? 'on' : ''}`} title={tv.online ? 'A player is connected' : 'No player connected'} />
        <strong className="grow ellipsis">{tv.name}</strong>
        <Badge>{tv.id}</Badge>
        <ConfirmBtn small kind="ghost" icon="trash" label="" confirm={`Remove “${tv.name}”?`} onConfirm={() => attempt(() => api.del(`/tvs/${tv.id}`), 'TV removed').then(onChange)} aria-label="Remove TV" />
      </div>
      <div className="row wrap">
        <Toggle checked={tv.power} onChange={(on) => send('power', { on })} label="Power" />
        <select className="select grow" value={tv.channel ?? ''} onChange={(e) => e.target.value && send('channel', { number: Number(e.target.value) })}>
          <option value="">Channel…</option>
          {channels.map((c) => <option key={c.id} value={c.number}>{c.number} · {c.name}</option>)}
        </select>
      </div>
      <div className="row">
        <Btn small icon={tv.muted ? 'mute' : 'vol'} aria-label={tv.muted ? 'Unmute' : 'Mute'} onClick={() => send('volume', { muted: !tv.muted })} />
        <input type="range" min={0} max={100} value={volume} onChange={(e) => setVolume(Number(e.target.value))} onPointerUp={() => send('volume', { level: volume })} onKeyUp={() => send('volume', { level: volume })} aria-label="Volume" />
        <span className="small dim" style={{ width: 28 }}>{volume}</span>
      </div>
      <div className="row wrap">
        <a className="btn small" href={href(`remote/${tv.id}`)}><Icon name="remote" /> Remote</a>
        <a className="btn small" href={`/?tv=${tv.id}`} target="_blank" rel="noreferrer"><Icon name="external" /> Open player</a>
      </div>
    </Card>
  );
}

export function Tvs() {
  const tvs = useQuery(() => api.get<Tv[]>('/tvs'));
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  useLiveEvent(['tv.state', 'tv.removed'], tvs.reload);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [id, setId] = useState('');
  const [touched, setTouched] = useState(false);
  const { attempt } = useToast();
  const idErr = id ? validateId(id) : null;

  const add = async () => {
    const r = await attempt(() => api.post('/tvs', { id, name: name.trim() || id }), 'TV added');
    if (r !== undefined) { setAdding(false); setName(''); setId(''); setTouched(false); tvs.reload(); }
  };

  return (
    <div className="page">
      <div className="page-head">
        <h1>TVs</h1>
        <span className="sub">Screens around the house, each on its own channel</span>
        <Btn kind="primary" icon="plus" className="right" onClick={() => setAdding(true)}>Add TV</Btn>
      </div>
      <Card className="flat">
        <p className="small dim" style={{ margin: 0 }}>
          Open <span className="mono">{location.origin}/?tv=<em>name</em></span> on a screen (a smart TV's browser, a tablet, a laptop) and it registers itself as that TV. Then change its channel from here or from the phone remote. Players that are connected show a green dot.
        </p>
      </Card>
      {tvs.loading && !tvs.data ? <Spinner /> : (tvs.data ?? []).length === 0 ? (
        <Card><Empty>No TVs yet.</Empty></Card>
      ) : (
        <div className="grid cols-2">
          {(tvs.data ?? []).map((t) => <TvCard key={t.id} tv={t} channels={channels.data ?? []} onChange={tvs.reload} />)}
        </div>
      )}
      {adding && (
        <Modal title="Add a TV" onClose={() => setAdding(false)} footer={<><Btn onClick={() => setAdding(false)}>Cancel</Btn><Btn kind="primary" disabled={!id || !!idErr} onClick={add}>Add</Btn></>}>
          <div className="stack">
            <Field label="Name"><input className="input" autoFocus placeholder="Living Room" value={name} onChange={(e) => { setName(e.target.value); if (!touched) setId(slugify(e.target.value)); }} /></Field>
            <Field label="Id" error={idErr} hint="Used in the player URL: /?tv=living-room"><input className="input mono" value={id} onChange={(e) => { setTouched(true); setId(e.target.value); }} /></Field>
          </div>
        </Modal>
      )}
    </div>
  );
}
