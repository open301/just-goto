import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
	root: fileURLToPath(new URL('./web', import.meta.url)),
	plugins: [tailwindcss()],
	server: {
		proxy: {
			'/api/': process.env.JUSTGOTO_API_URL || 'http://127.0.0.1:3000',
			'/swagger-ui': process.env.JUSTGOTO_API_URL || 'http://127.0.0.1:3000',
		},
	},
	build: {
		// Stable names keep the Rust include_bytes! paths explicit. The server disables caching.
		rollupOptions: {
			output: { entryFileNames: 'assets/app.js', assetFileNames: 'assets/app.[ext]' },
		},
	},
});
