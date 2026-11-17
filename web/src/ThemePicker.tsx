import { useEffect, useRef, useState } from 'react';
import { api } from './api';
import { cleanColor } from './guide';
import type { GraphicsStyle, ThemeCatalogue, ThemeChoice } from './types';
import { Field, Segmented } from './ui';

/** The themes the server offers (one list for every screen and every app). */
export function useThemeCatalogue() {
  const [cat, setCat] = useState<ThemeCatalogue | null>(null);
  useEffect(() => {
    let alive = true;
    api.get<ThemeCatalogue>('/theme').then((c) => alive && setCat(c)).catch(() => undefined);
    return () => { alive = false; };
  }, []);
  return cat;
}

export const emptyChoice = (): ThemeChoice => ({ theme: 'auto', colors: { primary: null, accent: null, background: null }, style: 'retro' });

/** Swatches for every theme, and the two colours to pick for a custom one. */
export function ThemePicker({ value, onChange, autoName, autoAbout }: {
  value: Pick<ThemeChoice, 'theme' | 'colors'>;
  onChange: (v: { theme: ThemeChoice['theme']; colors: ThemeChoice['colors'] }) => void;
  autoName?: string;
  autoAbout?: string;
}) {
  const cat = useThemeCatalogue();
  const themes = cat?.themes ?? [];
  const sel = themes.find((t) => t.id === value.theme);
  return (
    <div className="stack">
      <div className="row wrap" style={{ gap: 10 }}>
        {themes.map((t) => {
          const custom = t.id === 'custom';
          const a = custom ? cleanColor(value.colors.primary ?? '') ?? t.swatch[1] : t.swatch[0];
          const b = custom ? cleanColor(value.colors.accent ?? '') ?? t.swatch[2] : t.swatch[2];
          const mid = custom ? a : t.swatch[1];
          return (
            <button
              key={t.id}
              type="button"
              className="btn"
              aria-pressed={value.theme === t.id}
              title={t.id === 'auto' ? autoAbout ?? t.about : t.about}
              onClick={() => onChange({ theme: t.id, colors: value.colors })}
              style={{ display: 'grid', gap: 6, justifyItems: 'center', padding: 8, minWidth: 92, outline: value.theme === t.id ? '2px solid var(--accent)' : undefined }}
            >
              <span style={{ width: 64, height: 28, borderRadius: 6, background: `linear-gradient(110deg, ${a} 0 40%, ${mid} 40% 70%, ${b} 70% 100%)`, border: '1px solid var(--edge)' }} />
              <span className="small">{t.id === 'auto' ? autoName ?? t.name : t.name}</span>
            </button>
          );
        })}
      </div>
      {sel && <p className="small dim">{sel.id === 'auto' ? autoAbout ?? sel.about : sel.about}</p>}
      {value.theme === 'custom' && (
        <div className="row wrap nest" style={{ alignItems: 'flex-end' }}>
          {(['primary', 'accent', 'background'] as const).map((k) => (
            <Field key={k} label={k === 'primary' ? 'Main colour' : k === 'accent' ? 'Highlight' : 'Background'} hint={k === 'background' ? 'Blank = a dark shade of the main colour.' : undefined}>
              <span className="row" style={{ gap: 8 }}>
                <input type="color" aria-label={k} value={cleanColor(value.colors[k] ?? '') ?? (k === 'primary' ? '#3366ff' : k === 'accent' ? '#ffcc00' : '#101830')} onChange={(e) => onChange({ theme: value.theme, colors: { ...value.colors, [k]: e.target.value } })} />
                <input className="input mono" style={{ width: 110 }} placeholder="#3366ff" value={value.colors[k] ?? ''} onChange={(e) => onChange({ theme: value.theme, colors: { ...value.colors, [k]: e.target.value || null } })} />
              </span>
            </Field>
          ))}
        </div>
      )}
    </div>
  );
}

export function StylePicker({ value, onChange }: { value: GraphicsStyle; onChange: (s: GraphicsStyle) => void }) {
  const cat = useThemeCatalogue();
  const styles = cat?.styles ?? [{ id: 'retro' as const, name: 'Retro', about: '' }, { id: 'modern' as const, name: 'Modern', about: '' }];
  return (
    <Field label="Graphics style" hint={styles.find((s) => s.id === value)?.about}>
      <Segmented value={value} options={styles.map((s) => ({ value: s.id, label: s.name }))} onChange={onChange} />
    </Field>
  );
}

export type PreviewKind = 'card' | 'bumper' | 'ticker' | 'guide' | 'alert' | 'bar';
export const PREVIEWS: { value: PreviewKind; label: string }[] = [
  { value: 'card', label: 'Station card' },
  { value: 'bumper', label: 'Break card' },
  { value: 'guide', label: 'Guide' },
  { value: 'alert', label: 'Alert' },
  { value: 'bar', label: 'Alert bar' },
];

/** A picture of one of the station's graphics in a theme, redrawn a moment after each change. */
export function ThemePreview({ choice, what, id, name, number, scene, text, label, top }: { choice: ThemeChoice; what: PreviewKind; id?: string; name?: string; number?: number; scene?: string; text?: string; label?: string; top?: boolean }) {
  const [url, setUrl] = useState<string | null>(null);
  const last = useRef<string | null>(null);
  const key = JSON.stringify({ choice, what, id, name, number, scene, text, label, top });
  useEffect(() => {
    let live = true;
    const timer = setTimeout(async () => {
      try {
        const res = await fetch('/api/v1/theme/preview', {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ what, theme: choice, id, name, number, scene, text, label, top, t: 2 }),
          credentials: 'same-origin',
        });
        if (!res.ok || !live) return;
        const next = URL.createObjectURL(await res.blob());
        if (last.current) URL.revokeObjectURL(last.current);
        last.current = next;
        setUrl(next);
      } catch {
        /* a convenience */
      }
    }, 250);
    return () => { live = false; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return url ? <img src={url} alt={`The ${what} in this theme`} style={{ width: '100%', display: 'block', borderRadius: 8, aspectRatio: '16 / 9', background: '#000' }} /> : <div className="empty">Drawing…</div>;
}
