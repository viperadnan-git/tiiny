// Mirrors src/validate.rs and src/links/slug.rs (rules, messages, units: email in UTF-8 bytes, the rest in
// code points; whitespace is Unicode White_Space, not JS `\s`). The server stays authoritative.
import { shortHost } from './origin.ts';
import { z } from 'zod';

const trimWs = (s: string) => s.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, '');
const bytes = (s: string) => new TextEncoder().encode(s).length;
const codePoints = (s: string) => [...s].length;

export const passwordMin = 8;
const passwordMax = 128;
export const emailMax = 254;
const nameMax = 100;
const queryMax = 200;
export const urlMax = 2048;
const slugMin = 3;
export const slugMax = 64;
const slugChars = /^[A-Za-z0-9_-]+$/;

const label = /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/;
const badLocalChar = /[\p{White_Space}\p{Cc}]/u;

const isEmail = (e: string) => {
	const at = e.indexOf('@');
	const local = e.slice(0, at);
	const domain = e.slice(at + 1);
	return (
		at >= 0 &&
		bytes(e) <= emailMax &&
		local.length > 0 &&
		bytes(local) <= 64 &&
		!badLocalChar.test(local) &&
		domain.includes('.') &&
		domain.split('.').every((l) => label.test(l))
	);
};

const emailWith = (message: string) =>
	z
		.string()
		.transform((s) => trimWs(s).toLowerCase())
		.check(
			z.superRefine((e, ctx) => {
				if (!isEmail(e)) ctx.addIssue({ code: 'custom', message, input: e });
			})
		);
const email = emailWith('invalid email address');

const password = z.string().check(
	z.superRefine((pw, ctx) => {
		if (codePoints(pw) < passwordMin || codePoints(pw) > passwordMax)
			ctx.addIssue({
				code: 'custom',
				message: `password must be ${passwordMin}-${passwordMax} characters`,
				input: pw
			});
		else if (!trimWs(pw))
			ctx.addIssue({ code: 'custom', message: 'password must not be only whitespace', input: pw });
	})
);

const namePattern = /^\p{L}[\p{L}\p{M}'’ .-]*$/u;

/** Server takes `Option<String>`: null, missing and blank all mean "name is required". */
const name = z
	.string()
	.nullish()
	.transform((s) =>
		(s ?? '')
			.split(/\p{White_Space}+/u)
			.filter(Boolean)
			.join(' ')
	)
	.check(
		z.superRefine((n, ctx) => {
			const fail = (message: string) => ctx.addIssue({ code: 'custom', message, input: n });
			if (!n) fail('name is required');
			else if (codePoints(n) > nameMax) fail(`name must be at most ${nameMax} characters`);
			else if (!namePattern.test(n))
				fail(
					'name must start with a letter and contain only letters, spaces, apostrophes, periods and hyphens'
				);
		})
	);

/** Canonical unpadded base64url of 32 bytes: 43 chars, the last carrying 4 data bits + 2 zero bits. */
const token = z
	.string()
	.regex(/^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/, { message: 'invalid or expired token' });

const url = z.string().check(
	z.superRefine((raw, ctx) => {
		const fail = (message: string) => ctx.addIssue({ code: 'custom', message, input: raw });
		if (bytes(raw) > urlMax) return fail('url is too long');
		if (!raw.includes('://')) return fail('url must start with http:// or https://');
		let u: URL;
		try {
			u = new URL(raw);
		} catch {
			return fail('invalid url');
		}
		if (u.protocol !== 'http:' && u.protocol !== 'https:') return fail('url must be http or https');
		if (u.username || u.password) return fail('url must not contain credentials');
		if (!u.hostname) return fail('invalid url');
		// Self-links and TIINY_BLOCKED_DOMAINS depend on server config; the server checks those.
	})
);

const reservedSlugs = [
	'api',
	'admin',
	'dashboard',
	'login',
	'logout',
	'signup',
	'auth',
	'settings',
	'healthz',
	'_app',
	'static',
	'assets',
	'favicon.ico',
	'robots.txt',
	'features',
	'self-host',
	'privacy',
	'terms',
	'report'
];

const slug = z.string().check(
	z.superRefine((s, ctx) => {
		if (!slugChars.test(s) || s.length < slugMin || s.length > slugMax)
			ctx.addIssue({
				code: 'custom',
				message: "slug must be 3-64 characters of letters, digits, '-' or '_'",
				input: s
			});
		else if (reservedSlugs.includes(s))
			ctx.addIssue({ code: 'custom', message: 'slug is reserved', input: s });
	})
);

export const signupSchema = z.strictObject({ email, password, name });
/** Login never reveals a malformed email server-side (generic 401); the client still checks it. */
export const loginSchema = z.strictObject({
	email,
	password: z.string().min(1, 'enter your password')
});
export const forgotSchema = z.strictObject({ email });
export const resetSchema = z.strictObject({ token, password });
export const verifySchema = z.strictObject({ token });
export const updateNameSchema = z.strictObject({ name });
export const changePasswordSchema = z.strictObject({
	current_password: z.string().optional(),
	new_password: password
});
export const createLinkSchema = z.strictObject({
	url,
	slug: slug.optional(),
	permanent: z.boolean().optional()
});
export const updateLinkSchema = z.strictObject({
	url: url.optional(),
	slug: slug.optional(),
	permanent: z.boolean().optional()
});
export const reportReasons = ['phishing', 'malware', 'spam', 'illegal', 'other'] as const;
export const reportDetailsMax = 2000;
export const reportSchema = z.strictObject({
	slug: z.string(),
	reason: z.enum(reportReasons, { error: `reason must be one of ${reportReasons.join(', ')}` }),
	details: z
		.string()
		.nullish()
		.transform((s) => trimWs(s ?? '') || undefined)
		.check(
			z.superRefine((d, ctx) => {
				if (d && codePoints(d) > reportDetailsMax)
					ctx.addIssue({
						code: 'custom',
						message: `details must be at most ${reportDetailsMax} characters`,
						input: d
					});
			})
		),
	/** Blank means none. */
	contact_email: z.preprocess(
		(v) => (typeof v === 'string' && !trimWs(v) ? undefined : v),
		emailWith('contact email is invalid').optional()
	)
});
export const adminUserPatchSchema = z.strictObject({
	role: z.enum(['user', 'admin']).nullish(),
	disabled: z.boolean().nullish(),
	email_verified: z.boolean().nullish()
});
/** `query` param: trimmed, empty means no filter. */
export const searchQuerySchema = z
	.string()
	.transform((s) => trimWs(s) || undefined)
	.check(
		z.superRefine((q, ctx) => {
			if (q && codePoints(q) > queryMax)
				ctx.addIssue({
					code: 'custom',
					message: `query must be at most ${queryMax} characters`,
					input: q
				});
		})
	);

export type UpdateLinkBody = z.infer<typeof updateLinkSchema>;
export type AdminUserPatch = z.infer<typeof adminUserPatchSchema>;

/** Slug from a bare slug, `[http(s)://]<host>/<slug>[/][?q][#f]` (host incl. port, case-insensitive); null for anything else. */
export function slugFromInput(raw: string, host: string): string | null {
	const t = raw.trim();
	if (!t.includes('/')) return t;
	const m = /^(?:https?:\/\/)?([^/?#]+)\/([^/?#]+)\/?$/i.exec(t.replace(/[?#].*$/s, ''));
	return m && m[1].toLowerCase() === host.toLowerCase() ? m[2] : null;
}
export const slugOrLink = (s: typeof createLinkSchema.shape.slug) =>
	z
		.string()
		.transform((v, ctx) => {
			const r = slugFromInput(v, shortHost);
			if (r === null)
				ctx.addIssue({ code: 'custom', message: 'enter a slug or a link on this site', input: v });
			return r ?? v;
		})
		.pipe(s.unwrap())
		.optional();
export const createLinkFormSchema = createLinkSchema.extend({
	url: z.preprocess((v) => (typeof v === 'string' ? v.trim() : v), createLinkSchema.shape.url),
	slug: z.preprocess((v) => (v === '' ? undefined : v), slugOrLink(createLinkSchema.shape.slug))
});
export const updateLinkFormSchema = updateLinkSchema.extend({
	slug: slugOrLink(updateLinkSchema.shape.slug)
});
export const reportFormSchema = reportSchema.extend({
	slug: slugOrLink(reportSchema.shape.slug.min(1, 'enter the short link').optional()),
	reason: z.enum(reportReasons, { error: 'choose a reason' })
});
/** Forms that set a password add a client-only `confirm` field that must repeat it. */
const confirm = { confirm: z.string() };
const mismatch = { message: "the passwords don't match", path: ['confirm'] };
export const signupFormSchema = signupSchema
	.extend(confirm)
	.refine((d) => d.password === d.confirm, mismatch);
export const resetFormSchema = resetSchema
	.extend(confirm)
	.refine((d) => d.password === d.confirm, mismatch);
export const changePasswordFormSchema = changePasswordSchema
	.extend(confirm)
	.refine((d) => d.new_password === d.confirm, mismatch);
