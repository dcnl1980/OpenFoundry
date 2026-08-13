import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		proxy: {
			'/api': 'http://127.0.0.1:8080',
		},
	},
	preview: {
		proxy: {
			'/api': 'http://127.0.0.1:8080',
		},
	},
	test: {
		include: ['src/**/*.{test,spec}.ts'],
	},
});
