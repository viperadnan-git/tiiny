import { api } from '#lib/api-client.ts';

/** Open abuse reports, shown on the admin nav; admins only. */
export const openReports = $state({ count: 0 });

export const refreshOpenReports = () =>
	api<{ open: number }>('GET', '/admin/reports/count')
		.then((r) => (openReports.count = r.open))
		.catch(() => {});
