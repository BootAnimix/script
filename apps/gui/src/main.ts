import '@fontsource/ibm-plex-sans/400.css';
import '@fontsource/ibm-plex-sans/500.css';
import '@fontsource/ibm-plex-sans/600.css';
import '@fontsource/ibm-plex-mono/400.css';
import 'carbon-components-svelte/css/all.css';
import './app.css';
import App from './App.svelte';
import { mount } from 'svelte';

// Carbon ships five themes; the studio is a dark, dense desktop tool, so the
// Gray 100 theme is applied to the document root before the first paint.
document.documentElement.setAttribute('theme', 'g100');

const app = mount(App, {
  target: document.getElementById('app')!,
});

export default app;
