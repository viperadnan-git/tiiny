/// <reference types="node" />
import { readFileSync } from 'node:fs';
import tailwindcss from '@tailwindcss/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const version = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8').match(
	/^version = "(.+)"$/m
)?.[1];

export default defineConfig({
	define: {
		__REPO_URL__: JSON.stringify(
			process.env.PUBLIC_REPO_URL || 'https://github.com/viperadnan-git/tiiny'
		),
		__APP_VERSION__: JSON.stringify(version)
	},
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({ fallback: '200.html' }),
			// Canonical/og URLs hold the base-URL placeholder, which the crawler can't follow.
			prerender: { crawl: false },
			// The 404/410 pages and the SPA shell are served at arbitrary depths.
			paths: { relative: false }
		})
	],
	server: { proxy: { '/api': process.env.API_PROXY || 'http://localhost:8080' } }
});
