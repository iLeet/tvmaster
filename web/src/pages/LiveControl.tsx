import { useEffect, useRef, useState } from 'react';
import { api, errorMessage } from '../api';
import { formatClock } from '../lib';
import { useLiveEvent, useQuery } from '../live';
import type { ChannelView, LiveMode, LiveState } from '../types';
import { AirModal } from './ScheduleActions';
import { Badge, Btn, Card, Chips, ConfirmBtn, Field, Segmented, Spinner, useToast } from '../ui';

const MODES: { value: LiveMode; label: string; help: string }[] = [
  { value: 'test_screen', label: 'Test screen', help: 'Colour bars and a steady tone' },
  { value: 'off_air', label: 'Off air', help: 'The channel’s own off-air screen' },
  { value: 'black', label: 'Black', help: 'A black screen, no sound' },
  { value: 'static', label: 'Snow', help: 'Static and hiss' },
];
const modeLabel = (m: string) => (m === 'alert' ? 'Emergency alert' : MODES.find((x) => x.value === m)?.label ?? m);
const DURATIONS = [
  { value: 0, label: 'Until I clear it' },
  { value: 5, label: '5 minutes' },
  { value: 15, label: '15 minutes' },
  { value: 60, label: '1 hour' },
];

export function LiveControl() {
  const { say } = useToast();
  const channels = useQuery(() => api.get<ChannelView[]>('/channels'));
  const live = useQuery(() => api.get<LiveState>('/live'));
  useLiveEvent(['live.changed', 'channel.now'], () => { live.reload(); channels.reload(); });

  const list = channels.data ?? [];
  const overrides = live.data?.overrides ?? [];
  const byChannel = new Map(overrides.map((o) => [o.channel, o]));
  const samples = live.data?.samples ?? [];

  const [target, setTarget] = useState<string[]>([]); // empty = all channels
  const [event, setEvent] = useState('');
  const [message, setMessage] = useState('');
  const [minutes, setMinutes] = useState(0);
  const [busy, setBusy] = useState(false);
  const [airOn, setAirOn] = useState<ChannelView | null>(null);
  const [style, setStyle] = useState('full');
  const [theme, setTheme] = useState('retro');
  const [severity, setSeverity] = useState('emergency');
  const [sound, setSound] = useState('attention');
  const opts = live.data?.options;

  // A picture of the alert as it will look, redrawn as the choices change.
  const [shot, setShot] = useState<string | null>(null);
  const last = useRef<string | null>(null);
  useEffect(() => {
    let alive = true;
    const timer = setTimeout(async () => {
      try {
        const res = await fetch('/api/v1/live/preview', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ style, theme, severity, title: event, message, t: 1.2 }),
          credentials: 'same-origin',
        });
        if (!res.ok || !alive) return;
        const next = URL.createObjectURL(await res.blob());
        if (last.current) URL.revokeObjectURL(last.current);
        last.current = next;
        setShot(next);
      } catch {
        /* the preview is a convenience */
      }
    }, 250);
    return () => { alive = false; clearTimeout(timer); };
  }, [style, theme, severity, event, message]);

  const run = async (what: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await what();
      say(done);
      live.reload();
      channels.reload();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };
  const take = (channelIds: string[], mode: LiveMode, extra: object = {}, minutesFor = 0) =>
    api.post('/live', { channels: channelIds.length ? channelIds : ['all'], mode, ...(minutesFor ? { minutes: minutesFor } : {}), ...extra });
  const give = (channelIds: string[]) => api.post('/live/clear', { channels: channelIds.length ? channelIds : ['all'] });

  const scope = target.length === 0 ? 'every channel' : target.length === 1 ? (list.find((c) => c.id === target[0])?.name ?? target[0]) : `${target.length} channels`;
  const canAlert = event.trim().length > 0 && message.trim().length > 0;

  return (
    <div className="page">
      <div className="page-head">
        <h1>Live control</h1>
        <span className="sub">Take over any channel, or the whole station, right now</span>
      </div>
      {live.error && <div className="err-banner">{live.error}</div>}

      {overrides.length > 0 && (
        <div className="err-banner" style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
          <span><strong>{overrides.length === list.length ? 'The whole station is' : `${overrides.length} channel${overrides.length === 1 ? ' is' : 's are'}`} not showing normal programming.</strong></span>
          <span className="grow" />
          <Btn kind="primary" disabled={busy} onClick={() => run(() => give([]), 'Back to programming')}>Resume all programming</Btn>
        </div>
      )}

      <Card>
        <h3>Whole station</h3>
        <p className="dim small">One press changes every channel, guide included. Channels that are on air switch within a few seconds, and the choice survives a restart until you resume.</p>
        <div className="row wrap">
          {MODES.map((m) => (
            <ConfirmBtn key={m.value} label={`All to ${m.label.toLowerCase()}`} title={m.help} disabled={busy} confirm={`Put every channel on the ${m.label.toLowerCase()}? Nobody watching will see their programming until you resume it.`} onConfirm={() => run(() => take([], m.value), `Every channel: ${m.label.toLowerCase()}`)} />
          ))}
          <Btn kind="primary" disabled={busy || overrides.length === 0} onClick={() => run(() => give([]), 'Back to programming')}>Resume all programming</Btn>
        </div>
      </Card>

      <Card>
        <div className="row"><h3 style={{ margin: 0 }}>Emergency alert</h3><Badge tone="warn">Overrides programming</Badge></div>
        <p className="dim small">Take over the whole screen, or put a bar across the bottom of whatever is airing. Pick the look, how serious it is, and a sound (or none, for a quiet notice). The samples all say they are tests, so use them for a drill.</p>
        <div className="stack">
          <Field label="Start from a sample">
            <div className="row wrap">
              {samples.map((s) => (
                <Btn key={s.id} small onClick={() => { setEvent(s.event); setMessage(s.text); if (s.severity) setSeverity(s.severity); if (s.sound) setSound(s.sound); }}>{s.name}</Btn>
              ))}
            </div>
          </Field>
          <div className="row wrap" style={{ alignItems: 'flex-start' }}>
            <Field label="Shown as">
              <Segmented value={style} options={(opts?.styles ?? [{ value: 'full', label: 'Full screen' }, { value: 'bar', label: 'Bar across the bottom' }]).map((o) => ({ value: o.value, label: o.label }))} onChange={setStyle} />
            </Field>
            <Field label="Look">
              <select className="select" value={theme} onChange={(e) => setTheme(e.target.value)}>
                {(opts?.themes ?? [{ value: 'retro', label: 'Retro' }]).map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </Field>
            <Field label="How serious">
              <select className="select" value={severity} onChange={(e) => setSeverity(e.target.value)}>
                {(opts?.severities ?? [{ value: 'emergency', label: 'Emergency (red)' }]).map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </Field>
            <Field label="Sound" hint="Plays once when it goes on air.">
              <select className="select" value={sound} onChange={(e) => setSound(e.target.value)}>
                {(opts?.sounds ?? [{ value: 'attention', label: 'Attention tone' }]).map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </Field>
          </div>
          {style === 'bar' && <p className="small dim">The programme keeps playing underneath. If a channel has nothing playing (a stand-by screen, a bumper) the alert takes the whole screen instead.</p>}
          {shot && <img src={shot} alt="How the alert will look" style={{ width: '100%', maxWidth: 560, borderRadius: 10, border: '1px solid var(--edge)' }} />}
          <Field label="Event" hint="Short and big, like “Tornado warning”.">
            <input className="input" maxLength={80} value={event} onChange={(e) => setEvent(e.target.value)} placeholder="Emergency alert test" />
          </Field>
          <Field label="Message" hint={`${message.length} / 600`}>
            <textarea className="input" style={{ minHeight: 90 }} maxLength={600} value={message} onChange={(e) => setMessage(e.target.value)} placeholder="What people should know and do." />
          </Field>
          <div className="row wrap">
            <Field label="How long">
              <select className="select" value={minutes} onChange={(e) => setMinutes(Number(e.target.value))}>
                {DURATIONS.map((d) => <option key={d.value} value={d.value}>{d.label}</option>)}
              </select>
            </Field>
            <Field label="Where" hint={target.length === 0 ? 'Every channel' : ''}>
              <Chips options={list.map((c) => ({ value: c.id, label: `${c.number} ${c.name}` }))} value={target} onToggle={(id) => setTarget(target.includes(id) ? target.filter((x) => x !== id) : [...target, id])} />
            </Field>
          </div>
          <div className="row wrap">
            <ConfirmBtn kind="danger" icon="alert" label={`Put alert on ${scope}`} disabled={busy || !canAlert} confirm={`Put the emergency alert “${event.trim()}” on ${scope} now? It replaces whatever is airing until it ends or you resume programming.`} onConfirm={() => run(() => take(target, 'alert', { title: event.trim(), message: message.trim(), style, theme, severity, sound }, minutes), `Alert is on air on ${scope}`)} />
            {target.length > 0 && <Btn small onClick={() => setTarget([])}>Aim at every channel</Btn>}
          </div>
        </div>
      </Card>

      {airOn && <AirModal channel={airOn} onClose={() => setAirOn(null)} onDone={() => { channels.reload(); live.reload(); }} />}
      <h3>Channels</h3>
      {channels.loading && !channels.data ? <Spinner /> : (
        <div className="stack">
          {list.map((c) => {
            const o = byChannel.get(c.id);
            return (
              <Card key={c.id}>
                <div className="row wrap">
                  <span className="num-badge">{c.number}</span>
                  <div className="grow">
                    <div className="row">
                      <strong className="ellipsis">{c.name}</strong>
                      {o ? <Badge tone="warn">{modeLabel(o.mode)}{o.mode === 'alert' ? `: ${o.title}` : ''}</Badge> : <Badge tone="ok">Programming</Badge>}
                      <span className={`dot ${c.on_air ? 'on' : ''}`} title={c.on_air ? 'Streaming now' : 'Not streaming (starts when someone tunes in)'} />
                    </div>
                    <div className="small dim ellipsis">
                      {o ? (o.until_ms ? `Until ${formatClock(o.until_ms)}` : 'Until you resume') : c.now ? `Now: ${c.now.show ?? c.now.title}` : c.kind === 'guide' ? 'Guide channel' : 'Nothing scheduled right now'}
                    </div>
                  </div>
                  <div className="row wrap">
                    {MODES.map((m) => (
                      <Btn key={m.value} small title={m.help} disabled={busy || o?.mode === m.value} onClick={() => run(() => take([c.id], m.value), `${c.name}: ${m.label.toLowerCase()}`)}>{m.label}</Btn>
                    ))}
                    {c.kind === 'programming' && <Btn small icon="play" disabled={busy} onClick={() => setAirOn(c)}>Air now…</Btn>}
                    {c.kind === 'programming' && <Btn small icon="refresh" disabled={busy} title="Put the channel on the plan for this moment, even part way through a show" onClick={() => run(() => api.post(`/channels/${c.id}/refresh`), `${c.name} is on its plan`)}>Refresh</Btn>}
                    <Btn small icon="alert" onClick={() => { setTarget([c.id]); document.querySelector('.page')?.scrollTo?.({ top: 0 }); window.scrollTo({ top: 0, behavior: 'smooth' }); }}>Alert…</Btn>
                    {o && <Btn small kind="primary" disabled={busy} onClick={() => run(() => give([c.id]), `${c.name} is back to programming`)}>Resume</Btn>}
                    <span className="dim">|</span>
                    <Btn small disabled={busy} onClick={() => run(() => api.post(`/channels/${c.id}/stream`, { action: c.on_air ? 'restart' : 'start' }), c.on_air ? 'Restarted' : 'Started')}>{c.on_air ? 'Restart stream' : 'Start stream'}</Btn>
                    {c.on_air && <Btn small disabled={busy} onClick={() => run(() => api.post(`/channels/${c.id}/stream`, { action: 'stop' }), 'Stopped')}>Stop stream</Btn>}
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
