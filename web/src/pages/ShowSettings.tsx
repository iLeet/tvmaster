import { useState } from 'react';
import { audioMatches, langName, LANGUAGES, addLanguage, moveLanguage, removeLanguage, trackNote } from '../options';
import type { Aspect, Look, ShowOptions, ShowSummary, SubMode } from '../types';
import { Badge, Btn, Field, Segmented } from '../ui';

export const SUB_MODES: { value: SubMode; label: string; help: string }[] = [
  { value: 'off', label: 'Off', help: 'Never burn subtitles in.' },
  { value: 'foreign', label: 'When foreign', help: 'Subtitles only when the audio is not in one of your languages; otherwise just the forced ones (signs, foreign phrases).' },
  { value: 'always', label: 'Always', help: 'Always show subtitles in your language.' },
  { value: 'forced', label: 'Forced only', help: 'Only the forced subtitles inside the programme.' },
  { value: 'sdh', label: 'For the hard of hearing', help: 'Captions with sound cues and speaker names when the file has them.' },
];

export const LOOKS: { value: Look; label: string; help: string }[] = [
  { value: 'none', label: 'None', help: 'The picture as it is.' },
  { value: 'crt', label: 'CRT', help: 'Scanlines and a soft vignette, like a tube TV.' },
  { value: 'vhs', label: 'VHS', help: 'Noise, softness and washed colour, like a worn tape.' },
  { value: 'film', label: 'Film', help: 'Grain and warmth.' },
  { value: 'black_white', label: 'Black & white', help: 'Colour removed.' },
  { value: 'sepia', label: 'Sepia', help: 'An old-photograph tint.' },
];

export const ASPECTS: { value: Aspect; label: string; help: string }[] = [
  { value: 'fit', label: 'Fit', help: 'The whole picture, with bars where the shapes differ.' },
  { value: 'fill', label: 'Fill', help: 'Fill the screen, cropping the edges.' },
  { value: 'stretch', label: 'Stretch', help: 'Fill the screen by stretching (distorts).' },
];

/** An ordered list of languages with add, remove and reorder. */
export function LanguageList({ value, onChange, placeholder }: { value: string[]; onChange: (v: string[]) => void; placeholder: string }) {
  const [pick, setPick] = useState('');
  return (
    <div className="stack" style={{ gap: 8 }}>
      {value.length > 0 && (
        <div className="stack" style={{ gap: 4 }}>
          {value.map((l, i) => (
            <div key={l} className="row small">
              <span className="num-badge" style={{ minWidth: 26, height: 26 }}>{i + 1}</span>
              <span className="grow">{langName(l)} <span className="dim mono">{l}</span></span>
              <Btn small kind="ghost" icon="up" aria-label={`Move ${langName(l)} up`} disabled={i === 0} onClick={() => onChange(moveLanguage(value, i, -1))} />
              <Btn small kind="ghost" icon="down" aria-label={`Move ${langName(l)} down`} disabled={i === value.length - 1} onClick={() => onChange(moveLanguage(value, i, 1))} />
              <Btn small kind="ghost" icon="x" aria-label={`Remove ${langName(l)}`} onClick={() => onChange(removeLanguage(value, l))} />
            </div>
          ))}
        </div>
      )}
      <div className="row">
        <select className="select" style={{ minWidth: 200 }} value={pick} onChange={(e) => { if (e.target.value) onChange(addLanguage(value, e.target.value)); setPick(''); }} aria-label={placeholder}>
          <option value="">{value.length ? 'Add another language…' : placeholder}</option>
          {LANGUAGES.filter(([c]) => !value.includes(c)).map(([c, n]) => <option key={c} value={c}>{n}</option>)}
        </select>
      </div>
    </div>
  );
}

/**
 * One show's audio, subtitle, look and screen-shape settings. Used on the Shows
 * page (settings that follow the show onto every channel) and in a channel's
 * Picture & sound tab (an override for that channel only).
 */
export function ShowSettingsRow({
  show, ov, inheritLabel, inheritWhat, wantedAudio, onChange, badge, actions, check,
}: {
  show: ShowSummary;
  ov: ShowOptions;
  /** What "blank" means here: "the channel's" or "the channel's and the show's". */
  inheritLabel: string;
  inheritWhat: string;
  /** The audio languages that would apply if this row set none. */
  wantedAudio: string[];
  onChange: (p: ShowOptions) => void;
  badge?: string;
  /** Extra buttons beside Customise. */
  actions?: React.ReactNode;
  /** A tick box in front of the name. */
  check?: { on: boolean; set: (v: boolean) => void };
}) {
  const customised = Object.values(ov).some((v) => (Array.isArray(v) ? v.length > 0 : !!v));
  const [open, setOpen] = useState(false);
  const wanted = ov.audio_languages?.length ? ov.audio_languages : wantedAudio;
  return (
    <div className="block-card stack" style={{ gap: 8 }}>
      <div className="row wrap between">
        <div>
          {check && <input type="checkbox" checked={check.on} onChange={(e) => check.set(e.target.checked)} aria-label={`Select ${show.name}`} style={{ marginRight: 8 }} />}
          <strong>{show.name || '(no name)'}</strong> {customised && <Badge tone="accent">{badge ?? 'custom'}</Badge>}
          <div className="small dim">{trackNote(show)}</div>
          {!audioMatches(show, wanted) && <div className="small" style={{ color: 'var(--warn)' }}>None of its audio is in {wanted.map(langName).join(' or ')}; the file's default will play.</div>}
        </div>
        <span className="row">{actions}<Btn small onClick={() => setOpen(!open)}>{open ? 'Done' : customised ? 'Edit' : 'Customise'}</Btn></span>
      </div>
      {open && (
        <div className="stack nest" style={{ gap: 10 }}>
          <Field label="Audio languages" hint={`In order. Blank = ${inheritWhat}.`}>
            <LanguageList value={ov.audio_languages ?? []} onChange={(audio_languages) => onChange({ audio_languages })} placeholder={`Use ${inheritLabel}`} />
          </Field>
          <Field label="Subtitles" hint={ov.subtitles ? SUB_MODES.find((m) => m.value === ov.subtitles)?.help : `Blank = ${inheritWhat}. Pick Off for a show whose picture already has subtitles in it.`}>
            <Segmented value={(ov.subtitles ?? 'inherit') as SubMode | 'inherit'} options={[{ value: 'inherit', label: inheritLabel }, ...SUB_MODES.map((m) => ({ value: m.value, label: m.label }))]} onChange={(v) => onChange({ subtitles: v === 'inherit' ? null : (v as SubMode) })} />
          </Field>
          {ov.subtitles && ov.subtitles !== 'off' && (
            <Field label="Subtitle languages" hint={`In order. Blank = ${inheritWhat}.`}>
              <LanguageList value={ov.subtitle_languages ?? []} onChange={(subtitle_languages) => onChange({ subtitle_languages })} placeholder={`Use ${inheritLabel}`} />
            </Field>
          )}
          <Field label="Look">
            <Segmented value={(ov.look ?? 'inherit') as Look | 'inherit'} options={[{ value: 'inherit', label: inheritLabel }, ...LOOKS.map((l) => ({ value: l.value, label: l.label }))]} onChange={(v) => onChange({ look: v === 'inherit' ? null : (v as Look) })} />
          </Field>
          <Field label="Screen shape">
            <Segmented value={(ov.aspect ?? 'inherit') as Aspect | 'inherit'} options={[{ value: 'inherit', label: inheritLabel }, ...ASPECTS.map((a) => ({ value: a.value, label: a.label }))]} onChange={(v) => onChange({ aspect: v === 'inherit' ? null : (v as Aspect) })} />
          </Field>
        </div>
      )}
    </div>
  );
}
