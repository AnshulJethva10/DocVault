import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

// Production Content-Security-Policy. The app never talks to a server, so
// everything is locked to 'self'; 'wasm-unsafe-eval' is required to compile
// the WASM core and inline styles are used by React style props. Not applied
// in dev, where Vite's HMR client needs inline scripts and a websocket.
const CSP = [
    "default-src 'self'",
    "script-src 'self' 'wasm-unsafe-eval'",
    "worker-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "font-src 'self' data:",
    "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
].join('; ');

function contentSecurityPolicy(): Plugin {
    return {
        name: 'docvault-csp',
        apply: 'build',
        transformIndexHtml: () => [
            { tag: 'meta', attrs: { 'http-equiv': 'Content-Security-Policy', content: CSP }, injectTo: 'head-prepend' },
        ],
    };
}

export default defineConfig({
    plugins: [react(), wasm(), topLevelAwait(), contentSecurityPolicy()],
    worker: {
        format: 'es',
    },
    optimizeDeps: {
        exclude: ['docvault-core'],
    },
    base: './',
});
