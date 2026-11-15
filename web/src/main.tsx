import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { platformClass } from './lib';
import './styles.css';

document.documentElement.classList.add(platformClass(navigator.userAgent, navigator.platform));
// Inside the desktop shell the window itself is translucent (vibrancy / Mica).
if ((window as unknown as { __TVMASTER_SHELL__?: string }).__TVMASTER_SHELL__) {
  document.documentElement.classList.add('native-glass');
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// Installable app: cache the shell so it opens instantly and offline-tolerant.
if ('serviceWorker' in navigator && location.protocol !== 'file:') {
  addEventListener('load', () => {
    navigator.serviceWorker.register('./sw.js', { scope: './' }).catch(() => {});
  });
}
