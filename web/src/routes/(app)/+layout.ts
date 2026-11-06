import { api, getMe } from '#lib/api-client.ts';
import type { Providers } from '#lib/types.ts';
import type { LayoutLoad } from './$types';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ depends }) => {
	depends('app:auth');
	const [me, providers] = await Promise.all([getMe(), api<Providers>('GET', '/auth/providers')]);
	return { me, providers };
};
