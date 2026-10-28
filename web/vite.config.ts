import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';
import { fileURLToPath } from 'node:url';
const headers = {
	'Cross-Origin-Opener-Policy': 'same-origin',
	'Cross-Origin-Embedder-Policy': 'require-corp',
};
export default defineConfig({
	base: './',
	plugins: [svelte(), tailwindcss()],
	resolve: {
		alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) },
	},
	// These are lazy browser/worker imports. Prebundle them before the first question so
	// dependency discovery cannot reload the page and discard an in-memory connection.
	optimizeDeps: {
		include: [
			'@shadcn-svelte/primitives/message-scroller',
			'svelte-streamdown',
			'@earendil-works/pi-agent-core',
			'@earendil-works/pi-ai/api/openai-completions',
			'typebox',
			'phosphor-svelte/lib/ArrowDown',
			'phosphor-svelte/lib/Spinner',
		],
	},
	server: { headers, port: 8899, strictPort: true },
	preview: { headers, port: 8899, strictPort: true },
	build: { outDir: 'dist', emptyOutDir: true },
});
