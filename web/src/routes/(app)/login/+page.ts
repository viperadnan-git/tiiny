import { redirect } from '@sveltejs/kit';
import { safeNext } from '#lib/safe-next.ts';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent, url }) => {
	const { me } = await parent();
	const next = safeNext(url.searchParams.get('next'), url.origin);
	if (me) redirect(307, next ?? '/dashboard');
	return { next, error: url.searchParams.get('error') };
};
