import { useState } from 'react';
import { api, errorMessage } from '../api';
import { formatDuration, inputToMs, msToInput } from '../lib';
import { useQuery } from '../live';
import type { ChannelView, ShowSummary } from '../types';
import { Btn, Field, Modal, Segmented, Toggle, useToast } from '../ui';

interface Episode { id: number; season: number | null; episode: number | null; title: string | null; duration_ms: number }

type What = 'show' | 'episode' | 'random' | 'off_air';

/**
 * Choose what to air: a show's next episode, a particular episode, anything, or (for a stretch of the
 * schedule) nothing. With no `range` it airs right now; with one it replaces that stretch of the schedule.
 */
export function AirModal({ channel, range, tz, onClose, onDone }: { channel: ChannelView; range?: { from: number; to: number }; tz?: string; onClose: () => void; onDone: () => void }) {
  const { say } = useToast();
  const shows = useQuery(() => api.get<ShowSummary[]>(`/shows?library_ids=${channel.library_ids.join(',')}`), [channel.id]);
  const [what, setWhat] = useState<What>('show');
  const [show, setShow] = useState('');
  const [episode, setEpisode] = useState<number | null>(null);
  const [from, setFrom] = useState(range ? msToInput(range.from, tz) : '');
  const [to, setTo] = useState(range ? msToInput(range.to, tz) : '');
  const [busy, setBusy] = useState(false);
  const eps = useQuery(() => (what === 'episode' && show ? api.get<Episode[]>(`/channels/${channel.id}/episodes?show=${encodeURIComponent(show)}`) : Promise.resolve([] as Episode[])), [channel.id, what, show]);
  const list = shows.data ?? [];
  const chosen = show || list[0]?.name || '';

  const go = async () => {
    setBusy(true);
    try {
      const target: Record<string, unknown> =
        what === 'off_air' ? { off_air: true } : what === 'random' ? { random: true } : what === 'episode' ? { media_id: episode ?? eps.data?.[0]?.id } : { show: chosen };
      if (range) {
        const f = inputToMs(from, tz), t = inputToMs(to, tz);
        if (!(t > f)) throw new Error('The end must be after the start.');
        await api.post(`/channels/${channel.id}/schedule/edit`, { from_ms: f, to_ms: t, ...target });
        say(what === 'off_air' ? 'Left off the air' : 'Schedule changed');
      } else {
        await api.post(`/channels/${channel.id}/play`, target);
        say('On air now');
      }
      onDone();
      onClose();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={range ? 'Change what airs' : `Air something now on ${channel.name}`} onClose={onClose}
      footer={<><Btn onClick={onClose}>Cancel</Btn><Btn kind="primary" disabled={busy || (what !== 'random' && what !== 'off_air' && !chosen)} onClick={go}>{range ? 'Change the schedule' : 'Air it now'}</Btn></>}>
      <div className="stack">
        {!range && <p className="small dim">Cuts whatever is on (a stand-by screen, say) and airs this right away. The schedule carries on after it.</p>}
        <Field label="What">
          <Segmented value={what} onChange={setWhat} options={[
            { value: 'show', label: 'A show’s next episode' }, { value: 'episode', label: 'A particular episode' }, { value: 'random', label: 'Anything' },
            ...(range ? [{ value: 'off_air' as What, label: 'Nothing (off air)' }] : []),
          ]} />
        </Field>
        {(what === 'show' || what === 'episode') && (
          <Field label="Show">
            <select className="select" value={chosen} onChange={(e) => { setShow(e.target.value); setEpisode(null); }}>
              {list.map((s) => <option key={s.name} value={s.name}>{s.name} ({s.episodes} episodes)</option>)}
            </select>
          </Field>
        )}
        {what === 'episode' && (
          <Field label="Episode">
            <select className="select" value={episode ?? eps.data?.[0]?.id ?? ''} onChange={(e) => setEpisode(Number(e.target.value))}>
              {(eps.data ?? []).map((e) => (
                <option key={e.id} value={e.id}>{e.season != null && e.episode != null ? `S${String(e.season).padStart(2, '0')}E${String(e.episode).padStart(2, '0')} ` : ''}{e.title ?? ''} · {formatDuration(e.duration_ms)}</option>
              ))}
            </select>
          </Field>
        )}
        {range && (
          <div className="row wrap">
            <Field label="From" hint={tz ? `In ${tz}` : undefined}><input className="input" type="datetime-local" value={from} onChange={(e) => setFrom(e.target.value)} /></Field>
            <Field label="To"><input className="input" type="datetime-local" value={to} onChange={(e) => setTo(e.target.value)} /></Field>
          </div>
        )}
        {range && <p className="small dim">An episode longer than this stretch is cut where it ends; a shorter one is padded. It lasts until the schedule is re-rolled (editing the channel or pressing Re-roll plans it afresh).</p>}
        {!range && what === 'show' && <Toggle checked disabled onChange={() => undefined} label={<span className="small dim">The show carries on from the episode after this one.</span>} />}
      </div>
    </Modal>
  );
}
