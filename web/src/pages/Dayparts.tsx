import { useEffect, useState } from 'react';
import { api, errorMessage } from '../api';
import { move } from '../info';
import { useQuery } from '../live';
import { ThemeChoiceEditor, ThemePreview, kidsChoice, useThemeCatalogue } from '../ThemePicker';
import type { Daypart, DaypartState, Rating } from '../types';
import { Badge, Btn, Card, Chips, ConfirmBtn, Field, Segmented, Spinner, Toggle, useToast } from '../ui';

const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];

/** "6:00 AM", from "06:00". */
function twelve(t: string): string {
  const [h, m] = t.split(':').map(Number);
  if (Number.isNaN(h)) return t;
  return `${h % 12 === 0 ? 12 : h % 12}:${String(m).padStart(2, '0')} ${h < 12 ? 'AM' : 'PM'}`;
}

function when(p: Daypart): string {
  const days = p.days.length === 0 || p.days.length === 7 ? 'every day' : p.days.length === 5 && p.days.every((d) => d < 5) ? 'weekdays' : p.days.length === 2 && p.days.every((d) => d > 4) ? 'weekends' : p.days.map((d) => DAYS[d]).join(', ');
  return `${twelve(p.start)} – ${twelve(p.end)}, ${days}`;
}

const blank = (): Daypart => ({ name: 'New day part', days: [], start: '06:00', end: '09:00', channels: [], max_rating: null, allow_unrated: false, breaks: 'normal', look: null });

/**
 * Day parts: named stretches of the broadcast day, each with the strongest thing allowed to air in it, whether there are
 * commercial breaks, and a look of its own for the station's graphics (the kids look and gentle alerts in the morning, say).
 */
export function Dayparts() {
  const q = useQuery(() => api.get<DaypartState>('/dayparts'));
  const station = useThemeCatalogue()?.station;
  const { say } = useToast();
  const [parts, setParts] = useState<Daypart[] | null>(null);
  const [saved, setSaved] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (q.data && parts === null) { setParts(q.data.parts); setSaved(JSON.stringify(q.data.parts)); }
  }, [q.data, parts]);
  if (q.error) return <div className="page"><div className="err-banner">{q.error}</div></div>;
  if (!q.data || !parts) return <div className="page"><Spinner /></div>;
  const st = q.data;
  const dirty = JSON.stringify(parts) !== saved;
  const set = (i: number, p: Partial<Daypart>) => setParts(parts.map((x, k) => (k === i ? { ...x, ...p } : x)));
  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const r = await api.put<DaypartState>('/dayparts', { parts });
      setParts(r.parts);
      setSaved(JSON.stringify(r.parts));
      q.reload();
      say('Saved. Schedules are planned again, and channels on air pick up the looks.');
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };
  const ratingName = (r: Rating | null) => (r ? st.ratings.find((x) => x.id === r)?.name ?? r : 'No limit');

  return (
    <div className="page">
      <div className="page-head">
        <h1>Day parts</h1>
        <span className="sub">Kids in the morning, a darker look late at night</span>
        <span className="right row">
          {dirty && <Badge tone="warn">Unsaved</Badge>}
          <Btn kind="primary" disabled={!dirty || saving} onClick={save}>{saving ? <Spinner /> : 'Save'}</Btn>
        </span>
      </div>
      {error && <div className="err-banner">{error}</div>}
      <p className="dim">A day part is a stretch of the broadcast day ({st.timezone}) with its own rules: the strongest rating that may air, whether there are commercial breaks, and a look for the station’s graphics and alerts. The first one that covers a moment, for a channel, applies. Outside them, channels carry on as usual.</p>

      {parts.length === 0 && (
        <Card className="stack">
          <h2>Start with one</h2>
          <div className="grid cols-2">
            {st.presets.map((p) => (
              <button key={p.part.name} type="button" className="card clickable" style={{ textAlign: 'left' }} onClick={() => setParts([...parts, p.part])}>
                <strong>{p.part.name}</strong> <span className="small dim">{when(p.part)}</span>
                <div className="small dim">{p.about}</div>
              </button>
            ))}
          </div>
        </Card>
      )}

      {parts.map((p, i) => {
        const c = st.check[i];
        const saved = JSON.stringify(parts[i]) === JSON.stringify(q.data!.parts[i]);
        return (
          <Card key={i} className="stack">
            <div className="row wrap between">
              <span className="row wrap">
                <input className="input" style={{ width: 220, fontWeight: 600 }} aria-label="Name" value={p.name} maxLength={40} onChange={(e) => set(i, { name: e.target.value })} />
                <span className="small dim">{when(p)}</span>
              </span>
              <span className="row">
                <Btn small disabled={i === 0} onClick={() => setParts(move(parts, i, i - 1))} aria-label="Higher priority">↑</Btn>
                <Btn small disabled={i === parts.length - 1} onClick={() => setParts(move(parts, i, i + 1))} aria-label="Lower priority">↓</Btn>
                <ConfirmBtn small kind="danger" label="Remove" confirm={`Remove “${p.name}”? Channels go back to their usual programming in these hours.`} onConfirm={() => setParts(parts.filter((_, k) => k !== i))} />
              </span>
            </div>

            <div className="row wrap" style={{ alignItems: 'flex-end' }}>
              <Field label="From"><input className="input" type="time" value={p.start} onChange={(e) => e.target.value && set(i, { start: e.target.value })} /></Field>
              <Field label="Until" hint="Earlier than the start = runs overnight."><input className="input" type="time" value={p.end === '24:00' ? '00:00' : p.end} onChange={(e) => e.target.value && set(i, { end: e.target.value })} /></Field>
            </div>
            <Field label="Days" hint={p.days.length ? '' : 'Every day. Pick some to limit it.'}>
              <Chips options={DAYS.map((d, k) => ({ value: k, label: d }))} value={p.days} onToggle={(d) => set(i, { days: p.days.includes(d) ? p.days.filter((x) => x !== d) : [...p.days, d].sort() })} />
            </Field>
            <Field label="Channels" hint={p.channels.length ? 'Only these.' : 'Every TV channel. Pick some to limit it.'}>
              <Chips options={st.channels.map((ch) => ({ value: ch.id, label: `${ch.number} ${ch.name}` }))} value={p.channels} onToggle={(id) => set(i, { channels: p.channels.includes(id) ? p.channels.filter((x) => x !== id) : [...p.channels, id] })} />
            </Field>

            <div className="stack nest">
              <h3 style={{ margin: 0 }}>What may air</h3>
              <Field label="Strongest rating" hint="Shows rated above this are held back in these hours. Rate a show on the Shows page.">
                <select className="select" value={p.max_rating ?? ''} onChange={(e) => set(i, { max_rating: (e.target.value || null) as Rating | null })}>
                  <option value="">No limit</option>
                  {st.ratings.map((r) => <option key={r.id} value={r.id}>{r.name} ({r.about})</option>)}
                </select>
              </Field>
              {p.max_rating && (
                <>
                  <Toggle checked={p.allow_unrated} onChange={(allow_unrated) => set(i, { allow_unrated })} label={<span><strong>Let shows without a rating air</strong> <span className="dim small">off = only shows known to be suitable</span></span>} />
                  {saved && c && (
                    <p className="small" style={{ margin: 0 }}>
                      {c.airing} of {c.shows} shows can air as far as {ratingName(p.max_rating).toLowerCase()}.
                      {c.held_back > 0 && ` ${c.held_back} held back`}{c.unrated > 0 && ` (${c.unrated} ${c.unrated === 1 ? 'has' : 'have'} no rating)`}.
                      {c.airing === 0 && <strong> Nothing would air: rate some shows, or let unrated ones air.</strong>}
                    </p>
                  )}
                </>
              )}
              <Field label="Commercial breaks">
                <Segmented value={p.breaks} options={[{ value: 'normal', label: 'As the channel is set up' }, { value: 'off', label: 'None: shows run clean' }]} onChange={(breaks) => set(i, { breaks })} />
              </Field>
            </div>

            <div className="stack nest">
              <Toggle checked={!!p.look} onChange={(on) => set(i, { look: on ? (station ? { ...station } : kidsChoice()) : null })} label={<span><strong>A look of its own</strong> <span className="dim small">cards, bumpers, the bug, lower thirds, tickers and alerts in these hours</span></span>} />
              {p.look && (
                <>
                  <div className="row wrap"><Btn small onClick={() => set(i, { look: kidsChoice() })}>Use the kids look</Btn></div>
                  <ThemeChoiceEditor value={p.look} onChange={(look) => set(i, { look })} />
                  <div className="grid cols-2">
                    <ThemePreview choice={p.look} what="card" />
                    <ThemePreview choice={p.look} what="alert" />
                  </div>
                </>
              )}
            </div>
          </Card>
        );
      })}

      {parts.length > 0 && (
        <Card className="stack">
          <h3 style={{ margin: 0 }}>Add a day part</h3>
          <div className="row wrap">
            {st.presets.map((p) => <Btn key={p.part.name} small title={p.about} onClick={() => setParts([...parts, { ...p.part }])}>+ {p.part.name}</Btn>)}
            <Btn small onClick={() => setParts([...parts, blank()])}>+ Blank</Btn>
          </div>
        </Card>
      )}
    </div>
  );
}
