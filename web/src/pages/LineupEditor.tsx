import { useState } from 'react';
import { blockMinutes, DAY_NAMES, daysLabel, defaultBlock, formatDuration, timeZones, toggleDay } from '../lib';
import { GAPS, gapChoice, gapFill, hasSchedule, RUN_OUTS, runOutChoice, STRATEGIES } from '../programming';
import type { Block, ChannelOptions, EventDef, Lineup, RunOut, Season, ShowSummary, Source, Strategy } from '../types';
import { Btn, Card, Chips, Field, More, Segmented, Toggle } from '../ui';
import { OffAirEditor, SignOffEditor } from './OffAir';
import { WeekGrid } from './WeekGrid';

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
  playOnly = false,
}: {
  value: Source;
  onChange: (s: Source) => void;
  shows: ShowSummary[];
  /** A "finale" rotation can only contain single shows. */
  onlyShows?: boolean;
  /** Shows only (no chaos, no off-air screen): what "play these shows" chooses. */
  playOnly?: boolean;
}) {
  const types = onlyShows
    ? [{ value: 'show', label: 'One show' }]
    : playOnly
    ? [
        { value: 'show', label: 'One show' },
        { value: 'pool', label: 'Several shows' },
        { value: 'rotation', label: 'Rotation' },
      ]
    : [
        { value: 'show', label: 'One show' },
        { value: 'pool', label: 'Several shows' },
        { value: 'rotation', label: 'Rotation' },
        { value: 'chaos', label: 'Anything' },
        { value: 'off_air', label: 'Off-air screen' },
      ];
  const set = (type: string) => {
    if (type === value.type) return;
    if (type === 'show') onChange({ type: 'show', show: '' });
    else if (type === 'pool') onChange({ type: 'pool', shows: [], order: 'sequential' });
    else if (type === 'chaos') onChange({ type: 'chaos' });
    else if (type === 'off_air') onChange({ type: 'off_air' });
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
      {value.type === 'off_air' && <p className="small dim">The channel's off-air screen: title card, bars, snow or your picture. No programme plays.</p>}
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
      <Toggle checked={!!block.once} onChange={(once) => onChange({ ...block, once })} label={<span>One episode only <span className="dim small">air one at the start time, then the rest of the slot counts as unscheduled time (what you chose for “When nothing is scheduled”)</span></span>} />
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

const VIEW_KEY = 'tvmaster.blocks.view';

/** Blocks as a weekly grid (drag to edit) or a plain list. The choice is remembered. */
function BlocksPanel({ blocks, onChange, shows, step }: { blocks: Block[]; onChange: (b: Block[]) => void; shows: ShowSummary[]; step: number }) {
  const [view, setView] = useState<'grid' | 'list'>(() => {
    try {
      return localStorage.getItem(VIEW_KEY) === 'list' ? 'list' : 'grid';
    } catch {
      return 'grid';
    }
  });
  const pick = (v: 'grid' | 'list') => {
    setView(v);
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      /* remembering the choice is optional */
    }
  };
  return (
    <div className="stack">
      <div><Segmented value={view} options={[{ value: 'grid', label: 'Weekly grid' }, { value: 'list', label: 'List' }]} onChange={pick} /></div>
      {view === 'grid' ? (
        <WeekGrid
          blocks={blocks}
          onChange={onChange}
          step={step}
          renderEditor={(b, _i, set, remove) => <BlockEditor block={b} shows={shows} onChange={set} onRemove={remove} />}
        />
      ) : (
        <BlockList blocks={blocks} shows={shows} onChange={onChange} />
      )}
    </div>
  );
}

// ---------- the programming tab ----------

export interface Programming {
  strategy: Strategy;
  run_out: RunOut;
  chaos: number;
  lineup: Lineup;
  options: ChannelOptions;
}

/**
 * Everything about what plays when, in the order a person asks: how it plays, the week, what
 * happens in gaps, what happens when a show runs out, then the rarely-touched rest.
 */
export function LineupEditor({ value, onChange, shows, breaksSummary, onBreaks }: {
  value: Programming;
  onChange: (p: Partial<Programming>) => void;
  shows: ShowSummary[];
  breaksSummary: string;
  onBreaks: () => void;
}) {
  const [json, setJson] = useState<string | null>(null);
  const [jsonErr, setJsonErr] = useState<string | null>(null);
  const zones = timeZones();
  const { strategy, run_out: runOut, lineup, options } = value;
  const patch = (p: Partial<Lineup>) => onChange({ lineup: { ...lineup, ...p } });
  const weekly = hasSchedule(strategy, lineup);
  const gap = gapChoice(lineup, runOut);
  const seasonal = lineup.seasons.length;
  const specials = lineup.events.length;
  const setStrategy = (s: Strategy) => {
    // Choosing a weekly schedule means unscheduled time is a real gap: the off-air screen, unless a choice was made.
    const needsFill = s === 'block_grid' && lineup.fill === null && runOut !== 'off_air';
    onChange({ strategy: s, ...(needsFill ? { lineup: { ...lineup, fill: { type: 'off_air' as const } } } : {}) });
  };

  return (
    <div className="stack gap-lg">
      <datalist id="show-list">{shows.map((s) => <option key={s.name} value={s.name}>{`${s.episodes} episodes`}</option>)}</datalist>
      <datalist id="zone-list">{zones.map((z) => <option key={z} value={z} />)}</datalist>

      <Card className="stack">
        <h2>How it plays</h2>
        <Field label="Style" hint={STRATEGIES.find((s) => s.value === strategy)?.help}>
          <Segmented value={strategy} options={STRATEGIES} onChange={setStrategy} />
        </Field>
        {strategy !== 'block_grid' && weekly && <p className="small dim">This channel also has timed slots, which take priority at their times.</p>}
      </Card>

      {weekly && (
        <Card className="stack">
          <div>
            <h2>Your week</h2>
            <p className="small dim">Drag on the grid to place shows: “Weeknights at 8: sitcoms”, “Saturday morning cartoons”. Click a slot to choose what airs.</p>
          </div>
          <BlocksPanel blocks={lineup.blocks} shows={shows} step={lineup.grid_minutes ?? 30} onChange={(blocks) => patch({ blocks })} />
        </Card>
      )}

      {weekly && (
        <Card className="stack">
          <div>
            <h2>When nothing is scheduled</h2>
            <p className="small dim">Time between your slots, and slots whose show has no episodes left.</p>
          </div>
          <Field label="Then the channel will" hint={GAPS.find((g) => g.value === gap)?.help}>
            <Segmented value={gap} options={GAPS} onChange={(c) => patch({ fill: gapFill(c, strategy, runOut, lineup.fill) })} />
          </Field>
          {gap === 'these' && lineup.fill && <SourceEditor value={lineup.fill} shows={shows} playOnly onChange={(f) => patch({ fill: f })} />}
          {gap === 'screen' && <p className="small dim">Set what the screen looks like under “The off-air screen” below.</p>}
        </Card>
      )}

      <Card className="stack">
        <div>
          <h2>When a show runs out of episodes</h2>
          <p className="small dim">After its last unaired episode.</p>
        </div>
        <Field label="Then the channel will" hint={RUN_OUTS.find((r) => r.value === runOutChoice(runOut))?.help}>
          <Segmented value={runOutChoice(runOut)} options={RUN_OUTS} onChange={(r) => onChange({ run_out: r })} />
        </Field>
        {runOut === 'hybrid' && <p className="small dim">This channel was set to “hybrid”: something random for finished shows, and random shows in unscheduled time. Pick any choice above to replace that.</p>}
      </Card>

      <OffAirEditor options={options} onChange={(o) => onChange({ options: o })} />

      <Card className="row between wrap">
        <div>
          <strong>Commercial breaks</strong>
          <p className="small dim" style={{ margin: 0 }}>{breaksSummary}</p>
        </div>
        <Btn onClick={onBreaks}>Set up breaks…</Btn>
      </Card>

      <More
        summary={[
          strategy !== 'chaos' ? `${value.chaos}% surprises` : '',
          lineup.signoff ? 'signs off overnight' : '',
          seasonal ? `${seasonal} seasonal change${seasonal === 1 ? '' : 's'}` : '',
          specials ? `${specials} special${specials === 1 ? '' : 's'}` : '',
        ].filter(Boolean).join(' · ') || 'surprises, sign-off, seasons, holidays, time zone'}
      >
        {strategy !== 'chaos' && (
          <Field label={`Surprises: ${value.chaos}%`} hint="Swaps this share of picks for something random, so an orderly channel now and then airs something unexpected. 0 = never.">
            <input type="range" min={0} max={100} value={value.chaos} onChange={(e) => onChange({ chaos: Number(e.target.value) })} />
          </Field>
        )}
        <div className="row wrap" style={{ alignItems: 'flex-start' }}>
          <Field label="Time zone" hint="Every time on this channel is local to this zone.">
            <input className="input" list="zone-list" style={{ width: 240 }} value={lineup.timezone} placeholder="Server's time zone" onChange={(e) => patch({ timezone: e.target.value })} />
          </Field>
          <Field label="Start times" hint={weekly ? 'Shows start on this grid; spare time fills with breaks.' : 'Without a schedule, shows play back to back.'}>
            <select className="select" style={{ width: 200 }} value={lineup.grid_minutes ?? ''} onChange={(e) => patch({ grid_minutes: e.target.value ? Number(e.target.value) : null })}>
              <option value="">{weekly ? 'Automatic (every 30 minutes)' : 'Back to back'}</option>
              {[5, 10, 15, 20, 30, 60].map((m) => <option key={m} value={m}>Every {m} minutes</option>)}
            </select>
          </Field>
          <Field label="After the server was off" hint={lineup.catch_up === 'resume' ? 'Carry on from the last episode that really aired.' : 'Act as if the channel kept airing.'}>
            <Segmented value={lineup.catch_up} options={[{ value: 'resume', label: 'Carry on' }, { value: 'advance', label: 'Skip ahead' }]} onChange={(c) => patch({ catch_up: c })} />
          </Field>
        </div>

        <SignOffEditor lineup={lineup} onLineup={(l) => onChange({ lineup: l })} />

        <div className="stack">
          <div>
            <strong>Seasonal changes</strong>
            <p className="small dim">Date ranges whose slots replace the weekly ones: a fall lineup, summer reruns, a midseason premiere. (Called “seasons” in the API.)</p>
          </div>
          {lineup.seasons.map((s, i) => (
            <SeasonEditor key={i} season={s} shows={shows} step={lineup.grid_minutes ?? 30} onChange={(ns) => patch({ seasons: lineup.seasons.map((x, j) => (j === i ? ns : x)) })} onRemove={() => patch({ seasons: lineup.seasons.filter((_, j) => j !== i) })} />
          ))}
          <div><Btn icon="plus" onClick={() => patch({ seasons: [...lineup.seasons, { name: 'Fall lineup', from: '09-01', to: '05-31', blocks: [] }] })}>Add a seasonal change</Btn></div>
        </div>

        <div className="stack">
          <div>
            <strong>Holidays and specials</strong>
            <p className="small dim">Marathons, theme nights and yearly holiday specials. They override everything while they run, and keep their own episode counters. (Called “events” in the API.)</p>
          </div>
          {lineup.events.map((e, i) => (
            <EventEditor key={i} event={e} shows={shows} onChange={(ne) => patch({ events: lineup.events.map((x, j) => (j === i ? ne : x)) })} onRemove={() => patch({ events: lineup.events.filter((_, j) => j !== i) })} />
          ))}
          <div><Btn icon="plus" onClick={() => patch({ events: [...lineup.events, { name: 'Marathon', start: nextHour(), end: nextHour(6), yearly: false, source: { type: 'show', show: '' } }] })}>Add a special</Btn></div>
        </div>

        <div className="stack">
          <div className="row between">
            <strong>Advanced: the lineup as JSON</strong>
            <Btn small onClick={() => { setJson(json === null ? JSON.stringify(lineup, null, 2) : null); setJsonErr(null); }}>{json === null ? 'Show' : 'Hide'}</Btn>
          </div>
          {json !== null && (
            <>
              <textarea className="input mono" style={{ minHeight: 260 }} value={json} onChange={(e) => setJson(e.target.value)} spellCheck={false} />
              {jsonErr && <div className="err-banner">{jsonErr}</div>}
              <div><Btn onClick={() => { try { const v = JSON.parse(json) as Lineup; onChange({ lineup: { ...lineup, ...v } }); setJsonErr(null); } catch (e) { setJsonErr(`Not valid JSON: ${(e as Error).message}`); } }}>Apply JSON</Btn></div>
            </>
          )}
        </div>
      </More>
    </div>
  );
}

function nextHour(plus = 1): string {
  const d = new Date(Date.now() + plus * 3_600_000);
  d.setMinutes(0, 0, 0);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:00`;
}

function SeasonEditor({ season, onChange, onRemove, shows, step }: { season: Season; onChange: (s: Season) => void; onRemove: () => void; shows: ShowSummary[]; step: number }) {
  return (
    <div className="block-card stack">
      <div className="row wrap">
        <input className="input grow" placeholder="Season name" value={season.name} onChange={(e) => onChange({ ...season, name: e.target.value })} />
        <label className="row small dim">from <input className="input short" placeholder="MM-DD" value={season.from} onChange={(e) => onChange({ ...season, from: e.target.value })} /></label>
        <label className="row small dim">to <input className="input short" placeholder="MM-DD" value={season.to} onChange={(e) => onChange({ ...season, to: e.target.value })} /></label>
        <Btn kind="ghost" icon="trash" onClick={onRemove} aria-label="Remove season" />
      </div>
      <p className="small dim">Repeats every year. A range that ends before it starts wraps the new year (09-01 to 05-31).</p>
      <div className="nest"><BlocksPanel blocks={season.blocks} shows={shows} step={step} onChange={(blocks) => onChange({ ...season, blocks })} /></div>
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
