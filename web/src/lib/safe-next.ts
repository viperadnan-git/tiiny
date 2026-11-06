/** Same-origin relative path or null; guards every `?next=` against open redirects. */
export function safeNext(value: string | null | undefined, origin: string): string | null {
	if (!value || !value.startsWith('/') || value.startsWith('//') || value.startsWith('/\\'))
		return null;
	try {
		const u = new URL(value, origin);
		return u.origin === origin ? u.pathname + u.search + u.hash : null;
	} catch {
		return null;
	}
}

/** `path` carrying `next` along, if any. */
export const withNext = (path: string, next: string | null | undefined) =>
	next ? `${path}?next=${encodeURIComponent(next)}` : path;
