import { redirect } from '@sveltejs/kit';
import { withNext } from '#lib/safe-next.ts';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ parent, url }) => {
	const { me } = await parent();
	if (!me) redirect(307, withNext('/login', url.pathname + url.search));
	if (me.role !== 'admin') redirect(307, '/dashboard');
	return { me };
};
