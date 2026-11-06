import { createAvatar } from '@dicebear/core';
import * as rings from '@dicebear/rings';

const cache = new Map<string, Promise<string | null>>();

/** SHA-256 of the normalised email, hex. Null where `crypto.subtle` is missing (plain-http origins). */
async function emailHash(email: string): Promise<string | null> {
	if (!globalThis.crypto?.subtle) return null;
	const buf = await crypto.subtle.digest(
		'SHA-256',
		new TextEncoder().encode(email.trim().toLowerCase())
	);
	return [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** Rings drawn locally in Paper colours (stamp, ink, soft on sheet): a distinct ring mark per address, no network. */
export function avatarFor(email: string): Promise<string | null> {
	const key = email.trim().toLowerCase();
	let hit = cache.get(key);
	if (!hit) {
		hit = emailHash(key)
			.then((seed) =>
				seed
					? createAvatar(rings, {
							seed,
							ringColor: ['2f3fd0', '1a2421', '5d6b66'],
							backgroundColor: ['fbfcf8']
						}).toDataUri()
					: null
			)
			.catch(() => null);
		cache.set(key, hit);
	}
	return hit;
}

/** Up to two initials from a name, else from the email's local part. */
export function initials(name: string | null | undefined, email: string): string {
	return (name || email.split('@')[0] || '?')
		.split(/[\s.@_-]+/)
		.filter(Boolean)
		.slice(0, 2)
		.map((w) => [...w][0]?.toUpperCase())
		.join('');
}
