// Single source of this instance's origin on the client. No SvelteKit imports: bun scripts load it too.

/** Under bun `import.meta.env.SSR` is unset, so this reads `globalThis.location` (if any). */
const clientBase = () => {
	const meta = globalThis.document?.querySelector<HTMLMetaElement>(
		'meta[name=tiiny-base-url]'
	)?.content;
	return meta?.startsWith('http') ? meta : (globalThis.location?.origin ?? '');
};

/**
 * The server replaces the placeholder in every HTML file (including app.html's `tiiny-base-url` meta), so
 * client and prerendered head agree; the page origin when `TIINY_BASE_URL` is unset or under `vite dev`.
 */
export const baseUrl: string = import.meta.env.SSR ? '%%TIINY_BASE_URL%%' : clientBase();

/** Short links live on `baseUrl`, as the API's `short_url`. Prerendering has no origin: a stand-in. */
export const shortHost = import.meta.env.SSR || !baseUrl ? 'link.example' : new URL(baseUrl).host;
export const shortUrl = (slug: string) =>
	`${import.meta.env.SSR || !baseUrl ? 'https://link.example' : baseUrl}/${slug}`;
