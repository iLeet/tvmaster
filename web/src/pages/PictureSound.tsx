import { useState } from 'react';
import { emptyOptions, setShowOption, showOverride } from '../options';
import type { ChannelOptions, Corner, ShowSummary, System } from '../types';
import { Badge, Card, ConfirmBtn, Field, Segmented, Toggle } from '../ui';
import { OffAirEditor } from './OffAir';
import { PictureField } from '../PictureField';
import { ThemePreview, emptyChoice, useThemeCatalogue } from '../ThemePicker';
import { ASPECTS, LanguageList, LOOKS, ShowSettingsRow, SUB_MODES } from './ShowSettings';

const CORNERS: { value: Corner; label: string }[] = [
  { value: 'top_left', label: 'Top left' }, { value: 'top_right', label: 'Top right' },
  { value: 'bottom_left', label: 'Bottom left' }, { value: 'bottom_right', label: 'Bottom right' },
];

/** What the ticker will look like over a sample picture, in the channel's theme. */
function TickerPreview({ options }: { options: ChannelOptions }) {
  const station = useThemeCatalogue()?.station;
  const tk = options.overlay.ticker;
  const text = [...tk.lines, ...(tk.up_next ? ['Up next: Cheers at 8:30 PM'] : []), ...(tk.feeds.length ? ['Headlines from your feeds appear here'] : [])].filter((l) => l.trim()).join('   •   ');
  return <ThemePreview choice={options.theme ?? station ?? emptyChoice()} what="ticker" text={text} label={tk.label} top={tk.position === 'top'} />;
}

export function PictureSound({
  options, onChange, shows, sys, testOnly = false,
}: {
  options: ChannelOptions;
  onChange: (o: ChannelOptions) => void;
  shows: ShowSummary[];
  sys: System | undefined;
  /** A test-pattern channel only has an off-air screen to choose. */
  testOnly?: boolean;
}) {
  const o = options;
  const patch = <K extends keyof ChannelOptions>(k: K, v: Partial<ChannelOptions[K]>) => onChange({ ...o, [k]: { ...(o[k] as object), ...(v as object) } } as ChannelOptions);
  const ff = sys?.ffmpeg;
  const noLibass = ff && !ff.has_subtitles;
  const noDrawtext = ff && !ff.has_drawtext;
  const [showFilter, setShowFilter] = useState('');
  const shownShows = shows.filter((s) => s.name.toLowerCase().includes(showFilter.toLowerCase()));
  const staleLibraries = shows.some((s) => s.files_with_tracks < s.episodes);

  if (testOnly) return <div className="stack gap-lg"><OffAirEditor options={o} onChange={onChange} intro="What this channel shows, all the time. For a station card, add your own words; or use a picture of your own. (To make an ordinary channel go off the air at night, use its Programming tab.)" /></div>;

  return (
    <div className="stack gap-lg">
      <div className="row"><span className="small dim grow">Everything on this tab applies to this channel; per-show settings override it.</span>
        <ConfirmBtn small label="Reset to defaults" confirm="Audio, subtitles, picture and graphics go back to their defaults. Per-show settings are kept." onConfirm={() => onChange({ ...emptyOptions(), shows: o.shows })} /></div>
      <Card className="stack">
        <div><h2>Audio</h2><p className="small dim">Many files carry several audio tracks (original and dubbed languages, commentary). The channel picks one for every programme.</p></div>
        <Field label="Preferred languages" hint="In order. The first one a file has is used; if none match, the file's own default track plays. Commentary and audio-description tracks are skipped unless they are all there is.">
          <LanguageList value={o.audio.languages} onChange={(languages) => patch('audio', { languages })} placeholder="Any (the file's default)" />
        </Field>
        <div className="row wrap">
          <Toggle checked={o.audio.avoid_commentary} onChange={(v) => patch('audio', { avoid_commentary: v })} label="Skip commentary tracks" />
          <Toggle checked={o.audio.avoid_description} onChange={(v) => patch('audio', { avoid_description: v })} label="Skip audio description" />
        </div>
        <Field label="Volume levelling" hint={{ off: 'Leave levels as they are.', light: 'Gentle automatic levelling: cheap on the CPU.', broadcast: 'EBU R128 broadcast loudness: shows and commercials come out equally loud.' }[o.audio.loudness]}>
          <Segmented value={o.audio.loudness} options={[{ value: 'off', label: 'Off' }, { value: 'light', label: 'Light' }, { value: 'broadcast', label: 'Broadcast' }]} onChange={(loudness) => patch('audio', { loudness })} />
        </Field>
        <div className="row wrap">
          <Toggle checked={o.audio.night_mode} onChange={(v) => patch('audio', { night_mode: v })} label={<span>Night mode <span className="dim small">quieter explosions, clearer speech</span></span>} />
          <Toggle checked={o.audio.dialogue_boost} onChange={(v) => patch('audio', { dialogue_boost: v })} label={<span>Dialogue boost <span className="dim small">surround sources only</span></span>} />
          <Toggle checked={o.audio.mono} onChange={(v) => patch('audio', { mono: v })} label={<span>Mono <span className="dim small">one speaker, like an old set</span></span>} />
        </div>
      </Card>

      <Card className="stack">
        <div><h2>Subtitles</h2><p className="small dim">Subtitles are burned into the picture, so every TV, phone and IPTV app shows them. They come from the video file or from a subtitle file beside it (<span className="mono">Show S01E01.en.srt</span>).</p></div>
        {noLibass && (
          <div className="err-banner">This server's FFmpeg cannot draw text subtitles (it was built without libass). Subtitles that are pictures (many Blu-ray and DVD rips) still work. Install a full FFmpeg build for text ones; <span className="mono">tvmaster doctor</span> shows what yours has.</div>
        )}
        <Field label="When to show them" hint={SUB_MODES.find((m) => m.value === o.subtitles.mode)?.help}>
          <Segmented value={o.subtitles.mode} options={SUB_MODES} onChange={(mode) => patch('subtitles', { mode })} />
        </Field>
        {o.subtitles.mode !== 'off' && (
          <>
            <Field label="Subtitle languages" hint="In order. Blank = the same as your audio languages.">
              <LanguageList value={o.subtitles.languages} onChange={(languages) => patch('subtitles', { languages })} placeholder="Same as audio" />
            </Field>
            <div className="row wrap">
              <Field label="Size"><Segmented value={o.subtitles.size} options={[{ value: 'small', label: 'Small' }, { value: 'medium', label: 'Medium' }, { value: 'large', label: 'Large' }]} onChange={(size) => patch('subtitles', { size })} /></Field>
              <Field label="Style"><Segmented value={o.subtitles.style} options={[{ value: 'outline', label: 'Outline' }, { value: 'box', label: 'Caption box' }]} onChange={(style) => patch('subtitles', { style })} /></Field>
            </div>
          </>
        )}
      </Card>

      <Card className="stack">
        <div><h2>Picture</h2><p className="small dim">How everything on this channel is shaped and styled before it is streamed.</p></div>
        <div className="row wrap" style={{ alignItems: 'flex-start' }}>
          <Field label="Quality" hint="Lower sizes suit old TVs and slow networks.">
            <Segmented value={o.video.quality} options={[{ value: '480', label: '480p' }, { value: '720', label: '720p' }, { value: '1080', label: '1080p' }]} onChange={(quality) => patch('video', { quality })} />
          </Field>
          <Field label="Frame rate" hint="Blank = 30.">
            <input className="input num" type="number" min={15} max={60} placeholder="30" value={o.video.fps ?? ''} onChange={(e) => patch('video', { fps: e.target.value ? Number(e.target.value) : null })} />
          </Field>
          <Field label="Bitrate (kbit/s)" hint="Blank = the quality's default.">
            <input className="input num" type="number" min={300} max={40000} step={100} placeholder="auto" value={o.video.kbps ?? ''} onChange={(e) => patch('video', { kbps: e.target.value ? Number(e.target.value) : null })} />
          </Field>
        </div>
        <Field label="Screen shape" hint={ASPECTS.find((a) => a.value === o.video.aspect)?.help}>
          <Segmented value={o.video.aspect} options={ASPECTS} onChange={(aspect) => patch('video', { aspect })} />
        </Field>
        <Field label="Deinterlacing" hint="Old broadcast video is interlaced and shows combing on moving edges.">
          <Segmented value={o.video.deinterlace} options={[{ value: 'auto', label: 'When the file says so' }, { value: 'always', label: 'Always' }, { value: 'off', label: 'Never' }]} onChange={(deinterlace) => patch('video', { deinterlace })} />
        </Field>
        <Field label="Look" hint={LOOKS.find((l) => l.value === o.video.look)?.help}>
          <Segmented value={o.video.look} options={LOOKS} onChange={(look) => patch('video', { look })} />
        </Field>
      </Card>

      <Card className="stack">
        <div><h2>On-screen graphics</h2><p className="small dim">The little touches that make it feel like a station.</p></div>
        <Toggle checked={o.overlay.bug.enabled} onChange={(v) => patch('overlay', { bug: { ...o.overlay.bug, enabled: v } })} label={<span><strong>Channel logo</strong> <span className="dim">in a corner of the picture</span></span>} />
        {o.overlay.bug.enabled && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            <Field label="Text" hint="Blank = number and name. Ignored when a logo picture is set."><input className="input" style={{ width: 200 }} maxLength={24} placeholder="4 Retro TV" value={o.overlay.bug.text} onChange={(e) => patch('overlay', { bug: { ...o.overlay.bug, text: e.target.value } })} /></Field>
            <Field label="Logo picture" hint="Optional. Shown instead of the text; a transparent png works best."><PictureField value={o.overlay.bug.image} onChange={(image) => patch('overlay', { bug: { ...o.overlay.bug, image } })} /></Field>
            <Field label="Corner"><select className="select" value={o.overlay.bug.position} onChange={(e) => patch('overlay', { bug: { ...o.overlay.bug, position: e.target.value as Corner } })}>{CORNERS.map((c) => <option key={c.value} value={c.value}>{c.label}</option>)}</select></Field>
            <Field label={`Opacity ${o.overlay.bug.opacity}%`}><input type="range" min={10} max={100} value={o.overlay.bug.opacity} onChange={(e) => patch('overlay', { bug: { ...o.overlay.bug, opacity: Number(e.target.value) } })} /></Field>
          </div>
        )}
        <Toggle checked={o.overlay.show_title.enabled} onChange={(v) => patch('overlay', { show_title: { ...o.overlay.show_title, enabled: v } })} label={<span><strong>Now-playing card</strong> <span className="dim">the programme's name when it starts</span></span>} />
        {o.overlay.show_title.enabled && (
          <div className="nest"><Field label={`Stays up for ${o.overlay.show_title.seconds} seconds`}><input type="range" min={3} max={30} value={o.overlay.show_title.seconds} onChange={(e) => patch('overlay', { show_title: { ...o.overlay.show_title, seconds: Number(e.target.value) } })} /></Field></div>
        )}
        <Toggle checked={o.overlay.clock.enabled} onChange={(v) => patch('overlay', { clock: { ...o.overlay.clock, enabled: v } })} label={<span><strong>Clock</strong> <span className="dim">the time, drawn on the picture</span></span>} />
        {o.overlay.clock.enabled && (
          <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
            <Field label="Corner"><select className="select" value={o.overlay.clock.position} onChange={(e) => patch('overlay', { clock: { ...o.overlay.clock, position: e.target.value as Corner } })}>{CORNERS.map((c) => <option key={c.value} value={c.value}>{c.label}</option>)}</select></Field>
            <Toggle checked={o.overlay.clock.hour12} onChange={(v) => patch('overlay', { clock: { ...o.overlay.clock, hour12: v } })} label="12-hour" />
            {noDrawtext && <Badge tone="warn">needs FFmpeg with drawtext (this one has none, so no clock appears)</Badge>}
          </div>
        )}
      </Card>

      <Card className="stack">
        <div><h2>Ticker</h2><p className="small dim">A strip of scrolling text across the picture, like a news channel's crawl: station notices, headlines from a news feed, and what is on next.</p></div>
        <Toggle checked={o.overlay.ticker.enabled} onChange={(v) => patch('overlay', { ticker: { ...o.overlay.ticker, enabled: v } })} label={<strong>Show a ticker</strong>} />
        {o.overlay.ticker.enabled && (() => {
          const tk = o.overlay.ticker;
          const set = (p: Partial<typeof tk>) => patch('overlay', { ticker: { ...tk, ...p } });
          return (
            <div className="stack nest">
              <Field label="Messages" hint="One per line. They scroll one after another."><textarea className="input" rows={4} maxLength={4000} value={tk.lines.join('\n')} onChange={(e) => set({ lines: e.target.value.split('\n').slice(0, 20) })} /></Field>
              <Toggle checked={tk.up_next} onChange={(v) => set({ up_next: v })} label={<span>Add what is on next <span className="dim small">“Up next: Cheers at 8:30 PM”</span></span>} />
              <Field label="News feeds" hint="Addresses of RSS or Atom feeds, one per line. Their latest headlines are added and refreshed every 15 minutes."><textarea className="input mono" rows={2} value={tk.feeds.join('\n')} placeholder="https://example.com/news.xml" onChange={(e) => set({ feeds: e.target.value.split('\n').map((f) => f.trim()).filter(Boolean).slice(0, 5) })} /></Field>
              <div className="row wrap" style={{ alignItems: 'flex-end' }}>
                <Field label="Tab" hint="A word in a coloured tab at the left. Blank = none."><input className="input" style={{ width: 130 }} maxLength={16} value={tk.label} placeholder="NEWS" onChange={(e) => set({ label: e.target.value })} /></Field>
                <Field label="Where"><Segmented value={tk.position} options={[{ value: 'bottom', label: 'Bottom' }, { value: 'top', label: 'Top' }]} onChange={(position) => set({ position })} /></Field>
                <Field label="Speed"><Segmented value={tk.speed} options={[{ value: 'slow', label: 'Slow' }, { value: 'normal', label: 'Normal' }, { value: 'fast', label: 'Fast' }]} onChange={(speed) => set({ speed })} /></Field>
              </div>
              <Field label={tk.every_minutes === 0 ? 'On all the time' : `Appears every ${tk.every_minutes} minutes`} hint="Slide to 0 for a ticker that never goes away; otherwise it crosses the screen once each time.">
                <input type="range" min={0} max={60} step={5} value={tk.every_minutes} onChange={(e) => set({ every_minutes: Number(e.target.value) })} />
              </Field>
              <TickerPreview options={o} />
            </div>
          );
        })()}
      </Card>

      <Card className="stack">
        <div><h2>Per-show settings</h2><p className="small dim">Some shows want different treatment: anime in Japanese with English subtitles, a sitcom plain, a 1950s serial in black and white. A setting here applies on this channel only and beats both the channel's settings and anything set for the show on the Shows page.</p></div>
        {staleLibraries && <p className="small" style={{ color: 'var(--warn)' }}>Some files were scanned before audio and subtitle tracks were read. Re-scan the libraries (Libraries page) to see their languages.</p>}
        {shows.length > 8 && <input className="input" style={{ maxWidth: 280 }} placeholder="Filter shows…" value={showFilter} onChange={(e) => setShowFilter(e.target.value)} />}
        {shows.length === 0 && <p className="small dim">No shows in the libraries yet.</p>}
        <div className="stack">
          {shownShows.map((sh) => (
            <ShowSettingsRow
              key={sh.name}
              show={sh}
              ov={showOverride(o, sh.name)}
              inheritLabel="Default"
              inheritWhat={sh.defaults ? "the show's own settings (Shows page), then this channel's" : "this channel's settings"}
              wantedAudio={sh.defaults?.audio_languages?.length ? sh.defaults.audio_languages : o.audio.languages}
              badge="this channel"
              onChange={(p) => onChange(setShowOption(o, sh.name, p))}
            />
          ))}
        </div>
      </Card>
    </div>
  );
}
