import { api } from '../api';
import { useQuery } from '../live';
import { href } from '../router';
import type { ChannelTemplates } from '../types';
import { Card, Spinner } from '../ui';

const HEAD: Record<string, string> = {
  'TV channels': 'Plays shows from your libraries on a schedule.',
  Guide: 'Lists every channel, TV and info, with what is on now and next.',
  'Info channels': 'Nothing to schedule: pages that draw themselves.',
  'Test patterns': 'Colour bars, snow, a card or a picture, all day.',
};

/** The first step of a new channel: what kind is it? Each choice opens the editor with a sensible start. */
export function NewChannel() {
  const q = useQuery(() => api.get<ChannelTemplates>('/channel-templates'));
  if (q.error) return <div className="page"><div className="err-banner">{q.error}</div></div>;
  if (!q.data) return <div className="page"><Spinner /></div>;
  const groups = Array.from(new Set(q.data.templates.map((t) => t.group)));
  return (
    <div className="page">
      <div className="page-head">
        <a href={href('channels')} className="small">← Channels</a>
        <h1>New channel</h1>
        <span className="sub">What kind of channel is it?</span>
      </div>
      {groups.map((g) => (
        <div key={g} className="stack">
          <h2 style={{ marginBottom: 0 }}>{g} <span className="small dim" style={{ fontWeight: 400 }}>{HEAD[g] ?? ''}</span></h2>
          <div className="grid cols-2">
            {q.data!.templates.filter((t) => t.group === g).map((t) => (
              <a key={t.id} href={href(`channels/new?template=${t.id}`)} style={{ color: 'inherit', textDecoration: 'none' }}>
                <Card className="clickable">
                  <div className="row">
                    <span className="num-badge" title="The next free number on the dial">{t.suggested.number}</span>
                    <div className="grow">
                      <strong>{t.name}</strong>
                      <div className="small dim">{t.about}</div>
                    </div>
                  </div>
                </Card>
              </a>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
