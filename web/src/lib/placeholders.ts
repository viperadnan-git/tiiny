import type { AdminUserDetail, AdminUserRow, Link, ReportRow, RetiredSlug } from './types.ts';

// Stand-in data for skeletons: the real components render it invisibly, so loading rows have realistic widths.
const now = Math.floor(Date.now() / 1000);

export const placeholderLink: Link = {
	id: 0,
	user_id: 0,
	slug: 'xxxxxxx',
	url: 'https://example.com/a/fairly/long/destination/path',
	click_count: 0,
	permanent: false,
	blocked: false,
	created_at: now,
	updated_at: now
};

export const placeholderUser: AdminUserRow = {
	id: 0,
	email: 'someone@example.com',
	// Unverified is the common case (no SMTP), so the badge row is part of the typical row height.
	email_verified: false,
	name: 'Someone Example',
	role: 'user',
	disabled: false,
	has_password: true,
	created_at: now,
	link_count: 0
};

export const placeholderUserDetail: AdminUserDetail = {
	...placeholderUser,
	link_count: 12,
	click_count: 1234,
	providers: []
};

export const placeholderReport: ReportRow = {
	id: 0,
	slug: 'xxxxxxx',
	url: placeholderLink.url,
	reason: 'phishing',
	details: 'Asks visitors for a password.',
	// Contact is the rarer optional field; the typical report has details only.
	contact_email: null,
	status: 'open',
	action: null,
	created_at: now,
	resolved_at: null,
	link: {
		id: 0,
		slug: 'xxxxxxx',
		url: placeholderLink.url,
		owner_email: 'owner@example.com',
		blocked: false,
		click_count: 0
	},
	open_for_link: 1,
	blocked_link: false,
	disabled_owner: false
};

export const placeholderRetired: RetiredSlug = { id: 0, slug: 'xxxxxxx', retired_at: now };

/** Rows every list shows while loading or loading more. */
export const SKELETON_ROWS = 3;

/** `SKELETON_ROWS` copies of a placeholder with negative ids: keyed apart from real rows, and `id < 0` marks a skeleton row. */
export const skeletonRows = <T extends { id: number }>(p: T): T[] =>
	Array.from({ length: SKELETON_ROWS }, (_, i) => ({ ...p, id: -(i + 1) }));
