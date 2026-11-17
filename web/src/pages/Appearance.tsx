import { useEffect, useState } from 'react';
import { api, errorMessage } from '../api';
import { PREVIEWS, StylePicker, ThemePicker, ThemePreview, emptyChoice, useThemeCatalogue, type PreviewKind } from '../ThemePicker';
import type { ThemeChoice } from '../types';
import { Badge, Btn, Card, Segmented, Spinner, useToast } from '../ui';

/** One look for the whole station: the guide, the cards, the stand-by screen, alerts and on-screen graphics. */
export function Appearance() {
  const cat = useThemeCatalogue();
  const [draft, setDraft] = useState<ThemeChoice | null>(null);
  const [what, setWhat] = useState<PreviewKind>('card');
  const [saving, setSaving] = useState(false);
  const { say } = useToast();
  useEffect(() => { if (cat && !draft) setDraft(cat.station); }, [cat, draft]);
  if (!cat || !draft) return <div className="empty"><Spinner /></div>;
  const dirty = JSON.stringify(draft) !== JSON.stringify(cat.station);
  const save = async () => {
    setSaving(true);
    try {
      await api.put('/theme', draft);
      say('Saved. Channels on air pick it up within seconds.');
      location.reload();
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="stack gap-lg">
      <div className="page-head">
        <h1>Appearance</h1>
        <span className="right row">
          {dirty && <Badge tone="warn">Unsaved</Badge>}
          <Btn onClick={() => setDraft(emptyChoice())} disabled={JSON.stringify(draft) === JSON.stringify(emptyChoice())}>Reset</Btn>
          <Btn kind="primary" disabled={!dirty || saving} onClick={save}>{saving ? <Spinner /> : 'Save'}</Btn>
        </span>
      </div>
      <p className="dim">One look for everything the station draws itself: the guide, station and break cards, the stand-by and sign-off screens, alerts, the now-playing card, the channel logo and the clock. A channel can choose its own under its settings.</p>
      <Card className="stack">
        <h2>Theme</h2>
        <ThemePicker value={draft} onChange={(v) => setDraft({ ...draft, ...v })} autoName="Per channel" autoAbout="Colours made from each channel, so every channel has its own look." />
        <StylePicker value={draft.style} onChange={(style) => setDraft({ ...draft, style })} />
      </Card>
      <Card className="stack">
        <div className="row wrap between">
          <h2>Preview</h2>
          <Segmented value={what} options={PREVIEWS} onChange={setWhat} />
        </div>
        <ThemePreview choice={draft} what={what} />
      </Card>
    </div>
  );
}
