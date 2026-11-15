import { useState } from 'react';
import { blockMinutes, DAY_NAMES, daysLabel, defaultBlock, formatDuration, sourceLabel, timeZones, toggleDay } from '../lib';
import type { Block, EventDef, Lineup, Season, ShowSummary, Source } from '../types';
import { Btn, Card, Chips, Field, Segmented, Toggle } from '../ui';

// ---------- show picker ----------

function ShowTags({ value, shows, onChange }: { value: string[]; shows: ShowSummary[]; onChange: (v: string[]) => void }) {
  const [text, setText] = useState('');
  const add = (name: string) => {
    const n = name.trim();
    if (n && !value.some((v) => v.toLowerCase() === n.toLowerCase())) onChange([...value, n]);
    setText('');
  };
  return (
    <div className="stack" style={{ gap: 8 }}>
      {value.length > 0 && (
        <div className="chips">
          {value.map((v) => (
            <button key={v} type="button" className="chip on" title="Remove" onClick={() => onChange(value.filter((x) => x !== v))}>{v} ×</button>
          ))}
        </div>
      )}
      <input
        className="input"
        list="show-list"
        placeholder={value.length ? 'Add another show…' : 'All shows (type to pick specific ones)'}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          if (shows.some((s) => s.name === e.target.value)) add(e.target.value);
        }}
        onKeyDown={(e) => e.key === 'Enter' && (e.preventDefault(), add(text))}
        onBlur={() => text && add(text)}
      />
    </div>
  );
}

// ---------- source editor ----------

export function SourceEditor({
  value,
  onChange,
  shows,
  onlyShows = false,
}: {
  value: Source;
  onChange: (s: Source) => void;
  shows: ShowSummary[];
  /** A "finale" rotation can only contain single shows. */
  onlyShows?: boolean;
}) {
  const types = onlyShows
    ? [{ value: 'show', label: 'One show' }]
    : [
        { value: 'show', label: 'One show' },
        { value: 'pool', label: 'Several shows' },
        { value: 'rotation', label: 'Rotation' },
        { value: 'chaos', label: 'Chaos' },
      ];
  const set = (type: string) => {
    if (type === value.type) return;
    if (type === 'show') onChange({ type: 'show', show: '' });
    else if (type === 'pool') onChange({ type: 'pool', shows: [], order: 'sequential' });
    else if (type === 'chaos') onChange({ type: 'chaos' });
    else onChange({ type: 'rotation', advance: { weeks: 4 }, items: [{ type: 'show', show: '' }, { type: 'show', show: '' }] });
  };
  return (
    <div className="stack" style={{ gap: 8 }}>
      <Segmented value={value.type} options={types as { value: Source['type']; label: string }[]} onChange={set} />
      {value.type === 'show' && (
        <input className="input" list="show-list" placeholder="Show name" value={value.show} onChange={(e) => onChange({ type: 'show', show: e.target.value })} />
      )}
      {value.type === 'pool' && (
        <>
          <ShowTags value={value.shows} shows={shows} onChange={(s) => onChange({ ...value, shows: s })} />
          <Segmented value={value.order} options={[{ value: 'sequential', label: 'In order' }, { value: 'shuffle', label: 'Shuffled' }]} onChange={(order) => onChange({ ...value, order })} />
        </>
      )}
      {value.type === 'chaos' && <p className="small dim">Anything from the library, at random.</p>}
      {value.type === 'rotation' && (
        <div className="nest stack" style={{ gap: 8 }}>
          <div className="row wrap">
            <Segmented
              value={value.advance === 'finale' ? 'finale' : 'weeks'}
              options={[{ value: 'weeks', label: 'Swap every N weeks' }, { value: 'finale', label: 'Swap at each finale' }]}
              onChange={(k) => onChange({ ...value, advance: k === 'finale' ? 'finale' : { weeks: 4 }, items: k === 'finale' ? value.items.map((i) => (i.type === 'show' ? i : { type: 'show', show: '' })) : value.items })}
            />
            {value.advance !== 'finale' && (
              <label className="row small dim">every <input className="input num" type="number" min={1} max={52} value={value.advance.weeks} onChange={(e) => onChange({ ...value, advance: { weeks: Math.max(1, Number(e.target.value) || 1) } })} /> weeks</label>
            )}
          </div>
          {value.items.map((it, i) => (
            <div key={i} className="block-card">
              <div className="row between small dim" style={{ marginBottom: 6 }}>
                <span>Item {i + 1}</span>
                <Btn small kind="ghost" icon="x" aria-label="Remove item" disabled={value.items.length <= 1} onClick={() => onChange({ ...value, items: value.items.filter((_, j) => j !== i) })} />
              </div>
              <SourceEditor value={it} shows={shows} onlyShows={value.advance === 'finale'} onChange={(s) => onChange({ ...value, items: value.items.map((x, j) => (j === i ? s : x)) })} />
            </div>
          ))}
          <div><Btn small icon="plus" onClick={() => onChange({ ...value, items: [...value.items, { type: 'show', show: '' }] })}>Add item</Btn></div>
        </div>
      )}
    </div>
  );
}

// ---------- blocks ----------

function TimeField({ label, value, onChange, allow24 }: { label: string; value: string; onChange: (v: string) => void; allow24?: boolean }) {
  const midnight = value === '24:00';
  return (
    <Field label={label}>
      <div className="row">
        {midnight ? <span className="input" style={{ width: 120 }}>Midnight</span> : <input className="input" type="time" style={{ width: 130 }} value={value} onChange={(e) => onChange(e.target.value)} />}
        {allow24 && <Toggle checked={midnight} onChange={(m) => onChange(m ? '24:00' : '23:00')} label={<span className="small">to midnight</span>} />}
      </div>
    </Field>
  );
}

export function BlockEditor({ block, onChange, onRemove, shows }: { block: Block; onChange: (b: Block) => void; onRemove: () => void; shows: ShowSummary[] }) {
  const mins = blockMinutes(block);
  return (
    <div className="block-card stack">
      <div className="row wrap">
        <input className="input grow" placeholder="Name (e.g. Weeknight sitcoms)" value={block.name} onChange={(e) => onChange({ ...block, name: e.target.value })} />
        <Btn kind="ghost" icon="trash" onClick={onRemove} aria-label="Remove block" />
      </div>
      <Field label="Days" hint={daysLabel(block.days)}>
        <Chips options={DAY_NAMES.map((d, i) => ({ value: i, label: d }))} value={block.days} onToggle={(d) => onChange({ ...block, days: toggleDay(block.days, d) })} />
      </Field>
      <div className="row wrap">
        <TimeField label="Starts" value={block.start} onChange={(v) => onChange({ ...block, start: v })} />
        <TimeField label="Ends" value={block.end} onChange={(v) => onChange({ ...block, end: v })} allow24 />
        <span className="small dim" style={{ alignSelf: 'flex-end', paddingBottom: 9 }}>{mins ? formatDuration(mins * 60_000) : 'Start and end must differ'}</span>
      </div>
      <Field label="Airs"><SourceEditor value={block.source} onChange={(s) => onChange({ ...block, source: s })} shows={shows} /></Field>
    </div>
  );
}

function BlockList({ blocks, onChange, shows }: { blocks: Block[]; onChange: (b: Block[]) => void; shows: ShowSummary[] }) {
  return (
    <div className="stack">
      {blocks.map((b, i) => (
        <BlockEditor key={i} block={b} shows={shows} onChange={(nb) => onChange(blocks.map((x, j) => (j === i ? nb : x)))} onRemove={() => onChange(blocks.filter((_, j) => j !== i))} />
      ))}
      <div><Btn icon="plus" onClick={() => onChange([...blocks, defaultBlock()])}>Add block</Btn></div>
    </div>
  );
}

// ---------- the editor ----------

export function LineupEditor({ lineup, onChange, shows }: { lineup: Lineup; onChange: (l: Lineup) => void; shows: ShowSummary[] }) {
  const [json, setJson] = useState<string | null>(null);
  const [jsonErr, setJsonErr] = useState<string | null>(null);
  const zones = timeZones();
  const patch = (p: Partial<Lineup>) => onChange({ ...lineup, ...p });
  const hasBlocks = lineup.blocks.length > 0 || lineup.seasons.some((s) => s.blocks.length > 0);

  return (
    <div className="stack gap-lg">
      <datalist id="show-list">{shows.map((s) => <option key={s.name} value={s.name}>{`${s.episodes} episodes`}</option>)}</datalist>
      <datalist id="zone-list">{zones.map((z) => <option key={z} value={z} />)}</datalist>

      <Card className="flat stack">
        <h2>Basics</h2>
        <div className="row wrap" style={{ alignItems: 'flex-start' }}>
          <Field label="Time zone" hint="All times below are local to this zone.">
            <input className="input" list="zone-list" style={{ width: 240 }} value={lineup.timezone} placeholder="Server's time zone" onChange={(e) => patch({ timezone: e.target.value })} />
          </Field>
          <Field label="Time grid" hint={hasBlocks ? 'Shows start on this grid; spare time fills with breaks.' : 'Without blocks, shows play back to back.'}>
            <select className="select" style={{ width: 200 }} value={lineup.grid_minutes ?? ''} onChange={(e) => patch({ grid_minutes: e.target.value ? Number(e.target.value) : null })}>
              <option value="">{hasBlocks ? 'Automatic (30 minutes)' : 'None (back to back)'}</option>
              {[5, 10, 15, 20, 30, 60].map((m) => <option key={m} value={m}>{m} minutes</option>)}
            </select>
          </Field>
          <Field label="After downtime" hint={lineup.catch_up === 'resume' ? 'Carry on from the last episode that really aired.' : 'Act as if the channel kept airing.'}>
            <Segmented value={lineup.catch_up} options={[{ value: 'resume', label: 'Resume' }, { value: 'advance', label: 'Advance' }]} onChange={(c) => patch({ catch_up: c })} />
          </Field>
        </div>
        <Field label="When nothing is scheduled" hint="Programming for time no block covers. Off = follow the channel's strategy.">
          <Toggle checked={lineup.fill !== null} onChange={(on) => patch({ fill: on ? { type: 'chaos' } : null })} label="Custom fill programming" />
          {lineup.fill && <SourceEditor value={lineup.fill} shows={shows} onChange={(f) => patch({ fill: f })} />}
        </Field>
      </Card>

      <Card className="flat stack">
        <div>
          <h2>Weekly blocks</h2>
          <p className="small dim">Dayparts and time slots: "Weeknights 8pm: a rotation of sitcoms", "Saturday morning cartoons".</p>
        </div>
        <BlockList blocks={lineup.blocks} shows={shows} onChange={(blocks) => patch({ blocks })} />
      </Card>

      <Card className="flat stack">
        <div>
          <h2>Seasons</h2>
          <p className="small dim">Date ranges whose blocks replace the weekly ones: a fall lineup, summer reruns, a midseason premiere.</p>
        </div>
        {lineup.seasons.map((s, i) => (
          <SeasonEditor key={i} season={s} shows={shows} onChange={(ns) => patch({ seasons: lineup.seasons.map((x, j) => (j === i ? ns : x)) })} onRemove={() => patch({ seasons: lineup.seasons.filter((_, j) => j !== i) })} />
        ))}
        <div><Btn icon="plus" onClick={() => patch({ seasons: [...lineup.seasons, { name: 'Fall lineup', from: '09-01', to: '05-31', blocks: [] }] })}>Add season</Btn></div>
      </Card>

      <Card className="flat stack">
        <div>
          <h2>Events</h2>
          <p className="small dim">Marathons, theme nights and yearly holiday specials. They override everything while they run, and keep their own episode counters.</p>
        </div>
        {lineup.events.map((e, i) => (
          <EventEditor key={i} event={e} shows={shows} onChange={(ne) => patch({ events: lineup.events.map((x, j) => (j === i ? ne : x)) })} onRemove={() => patch({ events: lineup.events.filter((_, j) => j !== i) })} />
        ))}
        <div><Btn icon="plus" onClick={() => patch({ events: [...lineup.events, { name: 'Marathon', start: nextHour(), end: nextHour(6), yearly: false, source: { type: 'show', show: '' } }] })}>Add event</Btn></div>
      </Card>

      <Card className="flat stack">
        <div className="row between">
          <h2 style={{ margin: 0 }}>Advanced: lineup JSON</h2>
          <Btn small onClick={() => { setJson(json === null ? JSON.stringify(lineup, null, 2) : null); setJsonErr(null); }}>{json === null ? 'Show' : 'Hide'}</Btn>
        </div>
        {json !== null && (
          <>
            <textarea className="input mono" style={{ minHeight: 260 }} value={json} onChange={(e) => setJson(e.target.value)} spellCheck={false} />
            {jsonErr && <div className="err-banner">{jsonErr}</div>}
            <div><Btn onClick={() => { try { const v = JSON.parse(json) as Lineup; onChange({ ...lineup, ...v }); setJsonErr(null); } catch (e) { setJsonErr(`Not valid JSON: ${(e as Error).message}`); } }}>Apply JSON</Btn></div>
          </>
        )}
      </Card>
      <p className="small dim">Summary: {lineup.blocks.length} blocks. First block: {lineup.blocks[0] ? sourceLabel(lineup.blocks[0].source) : 'none'}.</p>
    </div>
  );
}

function nextHour(plus = 1): string {
  const d = new Date(Date.now() + plus * 3_600_000);
  d.setMinutes(0, 0, 0);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:00`;
}

function SeasonEditor({ season, onChange, onRemove, shows }: { season: Season; onChange: (s: Season) => void; onRemove: () => void; shows: ShowSummary[] }) {
  return (
    <div className="block-card stack">
      <div className="row wrap">
        <input className="input grow" placeholder="Season name" value={season.name} onChange={(e) => onChange({ ...season, name: e.target.value })} />
        <label className="row small dim">from <input className="input short" placeholder="MM-DD" value={season.from} onChange={(e) => onChange({ ...season, from: e.target.value })} /></label>
        <label className="row small dim">to <input className="input short" placeholder="MM-DD" value={season.to} onChange={(e) => onChange({ ...season, to: e.target.value })} /></label>
        <Btn kind="ghost" icon="trash" onClick={onRemove} aria-label="Remove season" />
      </div>
      <p className="small dim">Repeats every year. A range that ends before it starts wraps the new year (09-01 to 05-31).</p>
      <div className="nest"><BlockList blocks={season.blocks} shows={shows} onChange={(blocks) => onChange({ ...season, blocks })} /></div>
    </div>
  );
}

function EventEditor({ event, onChange, onRemove, shows }: { event: EventDef; onChange: (e: EventDef) => void; onRemove: () => void; shows: ShowSummary[] }) {
  return (
    <div className="block-card stack">
      <div className="row wrap">
        <input className="input grow" placeholder="Event name" value={event.name} onChange={(e) => onChange({ ...event, name: e.target.value })} />
        <Btn kind="ghost" icon="trash" onClick={onRemove} aria-label="Remove event" />
      </div>
      <div className="row wrap">
        <Field label="Starts"><input className="input" type="datetime-local" value={event.start} onChange={(e) => onChange({ ...event, start: e.target.value })} /></Field>
        <Field label="Ends"><input className="input" type="datetime-local" value={event.end} onChange={(e) => onChange({ ...event, end: e.target.value })} /></Field>
        <div style={{ alignSelf: 'flex-end', paddingBottom: 6 }}><Toggle checked={event.yearly} onChange={(y) => onChange({ ...event, yearly: y })} label="Repeat every year" /></div>
      </div>
      <Field label="Airs"><SourceEditor value={event.source} shows={shows} onChange={(s) => onChange({ ...event, source: s })} /></Field>
    </div>
  );
}
