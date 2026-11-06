import { goto } from '$app/navigation';
import { safeNext, withNext } from './safe-next.ts';
import type { User } from './types.ts';

/** 401 here is an expected answer, not an expired session. */
const authProbes = ['/auth/me', '/auth/login', '/auth/signup', '/auth/verify-email'];

export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string
	) {
		super(message);
	}
}

export async function api<T>(method: string, path: string, body?: unknown): Promise<T> {
	const res = await fetch(`/api/v1${path}`, {
		method,
		credentials: 'same-origin',
		headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});
	const text = await res.text();
	const json = text ? safeParse(text) : undefined;
	if (!res.ok) {
		if (res.status === 401 && !authProbes.includes(path.split('?')[0])) toLogin();
		const err = (json as { error?: { code?: string; message?: string } } | undefined)?.error;
		throw new ApiError(
			res.status,
			err?.code ?? `http_${res.status}`,
			err?.message ?? (res.statusText || 'Request failed')
		);
	}
	return json as T;
}

/** The signed-in user; null when signed out. */
export const getMe = () =>
	api<User>('GET', '/auth/me').catch((e: unknown) => {
		if (e instanceof ApiError && e.status === 401) return null;
		throw e;
	});

function safeParse(text: string): unknown {
	try {
		return JSON.parse(text);
	} catch {
		return undefined;
	}
}

/** Messages start with their field, lowercase ("url must…"): capitalize as a sentence (not in CSS, which would also capitalize a leading slug). */
export const sentence = (msg: string) =>
	/^url\b/.test(msg) ? 'URL' + msg.slice(3) : msg.charAt(0).toUpperCase() + msg.slice(1);

export function errorMessage(e: unknown): string {
	if (!(e instanceof ApiError)) return 'Couldn’t reach tiiny. Check your connection and try again.';
	return sentence(e.message);
}

export function qs(params: Record<string, string | number | null | undefined>): string {
	const q = new URLSearchParams();
	for (const [k, v] of Object.entries(params))
		if (v !== null && v !== undefined && v !== '') q.set(k, String(v));
	const s = q.toString();
	return s ? `?${s}` : '';
}

function toLogin() {
	if (location.pathname === '/login') return;
	const next = safeNext(location.pathname + location.search, location.origin);
	void goto(withNext('/login', next), { invalidateAll: true });
}
