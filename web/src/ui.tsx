// Small shared components: icons, buttons, form fields, modal, toasts.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from 'react';
import { errorMessage } from './api';

const ICONS: Record<string, string> = {
  home: 'M3 11l9-8 9 8v9a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z',
  tv: 'M4 6h16a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zm4 15h8M9 3l3 3 3-3',
  channels: 'M4 6h16M4 12h16M4 18h10',
  calendar: 'M5 5h14a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zm-1 5h16M8 3v4m8-4v4',
  folder: 'M3 7a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z',
  remote: 'M9 2h6a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1zm3 5v.01M12 12v.01M9.5 17h5',
  settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm7.4-3a7.4 7.4 0 0 0-.1-1.2l2-1.6-2-3.4-2.4 1a7 7 0 0 0-2-1.2L14.5 3h-4l-.4 2.6a7 7 0 0 0-2 1.2l-2.4-1-2 3.4 2 1.6a7.4 7.4 0 0 0 0 2.4l-2 1.6 2 3.4 2.4-1a7 7 0 0 0 2 1.2l.4 2.6h4l.4-2.6a7 7 0 0 0 2-1.2l2.4 1 2-3.4-2-1.6c.1-.4.1-.8.1-1.2z',
  plus: 'M12 5v14M5 12h14',
  clock: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2',
  palette: 'M12 3a9 9 0 1 0 0 18c1.1 0 1.8-.9 1.5-1.9-.3-1 .4-2.1 1.5-2.1H17a4 4 0 0 0 4-4c0-5-4-10-9-10zM7.5 11v.01M10 7v.01M14.5 7v.01M17 11v.01',
  alert: 'M12 3l10 18H2zM12 10v5M12 18v.01',
  live: 'M12 12m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0M7.8 7.8a6 6 0 0 0 0 8.4m8.4-8.4a6 6 0 0 1 0 8.4M4.9 4.9a10 10 0 0 0 0 14.2m14.2-14.2a10 10 0 0 1 0 14.2',
  trash: 'M4 7h16M10 11v6m4-6v6M6 7l1 13h10l1-13M9 7V4h6v3',
  refresh: 'M20 11a8 8 0 0 0-14-4L4 9m0-5v5h5M4 13a8 8 0 0 0 14 4l2-2m0 5v-5h-5',
  scan: 'M4 8V5a1 1 0 0 1 1-1h3m8 0h3a1 1 0 0 1 1 1v3m0 8v3a1 1 0 0 1-1 1h-3M8 20H5a1 1 0 0 1-1-1v-3M7 12h10',
  play: 'M7 4l13 8-13 8z',
  up: 'M6 14l6-6 6 6',
  down: 'M6 10l6 6 6-6',
  copy: 'M9 9h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1V10a1 1 0 0 1 1-1zM5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1',
  power: 'M12 3v9m6.4-5.4a9 9 0 1 1-12.8 0',
  mute: 'M11 5L6 9H3v6h3l5 4zM16 9l5 6m0-6l-5 6',
  vol: 'M11 5L6 9H3v6h3l5 4zM15.5 8.5a5 5 0 0 1 0 7M18.5 5.5a9 9 0 0 1 0 13',
  check: 'M5 12l5 5 9-10',
  x: 'M6 6l12 12M18 6L6 18',
  external: 'M14 4h6v6m0-6L10 14M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5',
};

export function Icon({ name }: { name: keyof typeof ICONS | string }) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={ICONS[name] ?? ''} />
    </svg>
  );
}

export function Btn({
  kind,
  small,
  icon,
  children,
  ...rest
}: {
  kind?: 'primary' | 'danger' | 'ghost';
  small?: boolean;
  icon?: string;
  children?: ReactNode;
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  const cls = ['btn', kind ?? '', small ? 'small' : '', !children ? 'icon' : ''].join(' ').trim();
  return (
    <button {...rest} className={`${cls} ${rest.className ?? ''}`}>
      {icon && <Icon name={icon} />}
      {children}
    </button>
  );
}

export function Card({ children, className = '', ...rest }: { children: ReactNode; className?: string } & React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div {...rest} className={`card ${className}`}>
      {children}
    </div>
  );
}

export function Field({ label, hint, error, children }: { label: string; hint?: string; error?: string | null; children: ReactNode }) {
  return (
    <div className="field">
      <label>{label}</label>
      {children}
      {error ? <div className="err">{error}</div> : hint ? <div className="hint">{hint}</div> : null}
    </div>
  );
}

export function Toggle({ checked, onChange, label, disabled }: { checked: boolean; onChange: (v: boolean) => void; label?: ReactNode; disabled?: boolean }) {
  return (
    <label className="toggle">
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span className="track" />
      {label && <span>{label}</span>}
    </label>
  );
}

export function Segmented<T extends string>({ value, options, onChange }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void }) {
  return (
    <div className="seg" role="group">
      {options.map((o) => (
        <button key={o.value} type="button" className={o.value === value ? 'on' : ''} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Chips<T extends string | number>({ options, value, onToggle }: { options: { value: T; label: string }[]; value: T[]; onToggle: (v: T) => void }) {
  return (
    <div className="chips">
      {options.map((o) => (
        <button key={String(o.value)} type="button" className={`chip ${value.includes(o.value) ? 'on' : ''}`} onClick={() => onToggle(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export const Badge = ({ children, tone = '' }: { children: ReactNode; tone?: '' | 'ok' | 'warn' | 'accent' }) => <span className={`badge ${tone}`}>{children}</span>;

export const Progress = ({ value, max }: { value: number; max: number }) => (
  <div className="bar" role="progressbar" aria-valuenow={value} aria-valuemax={max}>
    <i style={{ width: `${max > 0 ? Math.min(100, (value / max) * 100) : 0}%` }} />
  </div>
);

export const Spinner = () => <span className="spinner" role="status" aria-label="Loading" />;

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty">{children}</div>;
}

export function Modal({ title, onClose, children, footer }: { title: string; onClose: () => void; children: ReactNode; footer?: ReactNode }) {
  useEffect(() => {
    const on = (e: KeyboardEvent) => e.key === 'Escape' && onClose();
    addEventListener('keydown', on);
    return () => removeEventListener('keydown', on);
  }, [onClose]);
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title}>
        <div className="row between" style={{ marginBottom: 12 }}>
          <h2 style={{ margin: 0, fontSize: 18 }}>{title}</h2>
          <Btn kind="ghost" icon="x" onClick={onClose} aria-label="Close" />
        </div>
        {children}
        {footer && <div className="row" style={{ marginTop: 16, justifyContent: 'flex-end' }}>{footer}</div>}
      </div>
    </div>
  );
}

// ---- toasts ----

interface ToastItem { id: number; text: string; error: boolean }
const ToastCtx = createContext<{ say: (t: string, error?: boolean) => void }>({ say: () => {} });

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const say = useCallback((text: string, error = false) => {
    const id = Date.now() + Math.random();
    setItems((i) => [...i, { id, text, error }]);
    setTimeout(() => setItems((i) => i.filter((t) => t.id !== id)), error ? 7000 : 3500);
  }, []);
  return (
    <ToastCtx.Provider value={{ say }}>
      {children}
      <div className="toasts" aria-live="polite">
        {items.map((t) => (
          <div key={t.id} className={`toast ${t.error ? 'error' : ''}`}>{t.text}</div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

export function useToast() {
  const { say } = useContext(ToastCtx);
  return {
    say,
    /** Run an async action, report failures as an error toast. */
    attempt: async <T,>(fn: () => Promise<T>, okMessage?: string): Promise<T | undefined> => {
      try {
        const r = await fn();
        if (okMessage) say(okMessage);
        return r;
      } catch (e) {
        say(errorMessage(e), true);
        return undefined;
      }
    },
  };
}

/** A button that asks before doing something destructive. */
export function ConfirmBtn({ label, confirm, onConfirm, ...rest }: { label: ReactNode; confirm: string; onConfirm: () => void } & Omit<React.ComponentProps<typeof Btn>, 'onClick' | 'children'>) {
  const [asking, setAsking] = useState(false);
  return (
    <>
      <Btn {...rest} onClick={() => setAsking(true)}>{label}</Btn>
      {asking && (
        <Modal
          title="Are you sure?"
          onClose={() => setAsking(false)}
          footer={
            <>
              <Btn onClick={() => setAsking(false)}>Cancel</Btn>
              <Btn kind="danger" onClick={() => { setAsking(false); onConfirm(); }}>{typeof label === 'string' ? label : 'Confirm'}</Btn>
            </>
          }
        >
          <p>{confirm}</p>
        </Modal>
      )}
    </>
  );
}

export function CopyField({ label, value }: { label: string; value: string }) {
  const { say } = useToast();
  return (
    <Field label={label}>
      <div className="row">
        <input className="input mono" readOnly value={value} onFocus={(e) => e.target.select()} />
        <Btn
          icon="copy"
          aria-label={`Copy ${label}`}
          onClick={() => navigator.clipboard?.writeText(value).then(() => say('Copied'), () => say('Copy failed', true))}
        />
      </div>
    </Field>
  );
}
