import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { optimizeCss } from 'carbon-preprocess-svelte';

/**
 * Carbon's precompiled theme CSS embeds @font-face rules that point at IBM's
 * CDN. The studio is a desktop app: it must not fetch fonts at runtime, and
 * the Tauri CSP would block them anyway. IBM Plex is bundled locally through
 * @fontsource, so the remote declarations are removed at transform time.
 */
function stripRemoteCarbonFonts(): Plugin {
  return {
    name: 'strip-carbon-remote-fonts',
    enforce: 'pre',
    transform(code, id) {
      if (!id.includes('carbon-components-svelte') || !id.includes('.css')) return null;
      return {
        code: code.replace(/@font-face\s*\{[^}]*s81c\.com[^}]*\}/g, ''),
        map: null,
      };
    },
  };
}

export default defineConfig({
  plugins: [stripRemoteCarbonFonts(), svelte(), optimizeCss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: false,
    host: '127.0.0.1',
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
  },
});
