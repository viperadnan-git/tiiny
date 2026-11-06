export type Role = 'user' | 'admin';
export type OauthProvider = 'google' | 'github';
export type StatsRange = '7d' | '30d' | '90d' | 'all';

export interface User {
	id: number;
	email: string;
	email_verified: boolean;
	name: string | null;
	role: Role;
	disabled: boolean;
	has_password: boolean;
	created_at: number;
}

export interface AdminUserRow extends User {
	link_count: number;
}

export interface AdminUserDetail extends AdminUserRow {
	click_count: number;
	providers: string[];
}

export interface Providers {
	password: boolean;
	google: boolean;
	github: boolean;
	email_verification: boolean;
	signup: boolean;
	guest_links: boolean;
}

export interface Link {
	id: number;
	user_id: number | null;
	slug: string;
	url: string;
	click_count: number;
	permanent: boolean;
	blocked: boolean;
	created_at: number;
	updated_at: number;
}

export interface LinkPage {
	links: Link[];
	next_cursor: number | null;
}

export interface UserPage {
	users: AdminUserRow[];
	next_cursor: number | null;
}

export interface DayCount {
	day: number;
	count: number;
}

export interface ValueCount {
	value: string;
	count: number;
}

export interface LinkStats {
	click_count: number;
	series: DayCount[];
	referrers: ValueCount[];
	countries: ValueCount[];
	devices: ValueCount[];
	browsers: ValueCount[];
	oses: ValueCount[];
}

export interface Account {
	provider: OauthProvider;
}

export type ReportStatus = 'open' | 'actioned' | 'dismissed';
export type ReportAction = 'block' | 'delete' | 'disable_owner' | 'dismiss';

export interface ReportRow {
	id: number;
	slug: string;
	url: string;
	reason: string;
	details: string | null;
	contact_email: string | null;
	status: ReportStatus;
	/** Outcome once actioned: `blocked`, `deleted` or `owner_disabled`. */
	action: string | null;
	created_at: number;
	resolved_at: number | null;
	link: {
		id: number;
		slug: string;
		url: string;
		owner_email: string | null;
		blocked: boolean;
		click_count: number;
	} | null;
	open_for_link: number;
	/** Actioned reports: what reopening undoes (false when it was already so). */
	blocked_link: boolean;
	disabled_owner: boolean;
}

export interface ReportPage {
	reports: ReportRow[];
	next_cursor: number | null;
}

export interface RetiredSlug {
	id: number;
	slug: string;
	retired_at: number;
}

export interface RetiredSlugPage {
	slugs: RetiredSlug[];
	next_cursor: number | null;
}
