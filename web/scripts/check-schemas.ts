import './fake-location.ts';
import {
	adminUserPatchSchema,
	changePasswordFormSchema,
	changePasswordSchema,
	createLinkFormSchema,
	createLinkSchema,
	slugFromInput,
	updateLinkFormSchema,
	forgotSchema,
	loginSchema,
	reportSchema,
	reportFormSchema,
	resetFormSchema,
	resetSchema,
	searchQuerySchema,
	signupFormSchema,
	signupSchema,
	updateLinkSchema,
	updateNameSchema,
	verifySchema
} from '../src/lib/schemas.ts';
import { z } from 'zod';

const fieldErrors = (error: z.ZodError) =>
	z.flattenError(error).fieldErrors as Partial<Record<string, string[]>>;

let cases = 0;
const fail = (what: string, v: unknown) => {
	console.error('FAIL', what, JSON.stringify(v));
	process.exit(1);
};
const ok = (what: string, cond: boolean, v: unknown) => {
	cases++;
	if (!cond) fail(what, v);
};
type Schema = { safeParse: (v: unknown) => { success: boolean; data?: unknown; error?: unknown } };
const parsed = (s: Schema, v: unknown) => {
	const r = s.safeParse(v);
	return r.success
		? { data: r.data as Record<string, unknown>, errors: null }
		: { data: null, errors: fieldErrors(r.error as never) };
};
const firstError = (s: Schema, v: unknown, field: string) =>
	parsed(s, v).errors?.[field]?.[0] ?? null;

const pw = 'password1';
const signup = (over: Record<string, unknown>) => ({
	email: 'a@b.co',
	password: pw,
	name: 'Ann',
	...over
});

// Names (signup and PATCH /me share one rule).
const goodNames: [string, string][] = [
	['José', 'José'],
	['Zoë', 'Zoë'],
	['Ngô Bảo Châu', 'Ngô Bảo Châu'],
	["O'Brien", "O'Brien"],
	['O’Brien', 'O’Brien'],
	['D’Angelo', 'D’Angelo'],
	['Jean-Luc', 'Jean-Luc'],
	['A. Smith', 'A. Smith'],
	['  Ada \t Lovelace ', 'Ada Lovelace'],
	['Ada\u0085Lovelace', 'Ada Lovelace'],
	['émile', 'émile'],
	['a'.repeat(100), 'a'.repeat(100)],
	['𝒜'.repeat(100), '𝒜'.repeat(100)]
];
for (const [raw, want] of goodNames) {
	ok('name accepted', parsed(signupSchema, signup({ name: raw })).data?.name === want, raw);
	ok('patch name accepted', parsed(updateNameSchema, { name: raw }).data?.name === want, raw);
}
for (const raw of [
	'dev123',
	'😀',
	'-Bob',
	"'Bob",
	'Bob!',
	'Bob_Smith',
	'a'.repeat(101),
	'Ada\uFEFF',
	'\uFEFFAda'
])
	ok(
		'name rejected',
		firstError(signupSchema, signup({ name: raw }), 'name') !== null &&
			firstError(signupSchema, signup({ name: raw }), 'name') !== 'name is required',
		raw
	);
for (const raw of [null, undefined, '', ' ', ' \u0085\u3000 ']) {
	ok(
		'blank name',
		firstError(signupSchema, signup({ name: raw }), 'name') === 'name is required',
		raw
	);
	ok(
		'blank patch name',
		firstError(updateNameSchema, { name: raw }, 'name') === 'name is required',
		raw
	);
}
ok('missing patch name', firstError(updateNameSchema, {}, 'name') === 'name is required', {});

// Emails (UTF-8 byte lengths, White_Space trim, lowercase).
const max = `a@${'b'.repeat(63)}.${'c'.repeat(63)}.${'d'.repeat(120)}.com`;
const goodEmails: [string, string][] = [
	['a@b.co', 'a@b.co'],
	[' Foo@Example.COM\u0085', 'foo@example.com'],
	['a@xn--bcher-kva.example', 'a@xn--bcher-kva.example'],
	[`${'a'.repeat(64)}@x.com`, `${'a'.repeat(64)}@x.com`],
	[`${'é'.repeat(32)}@x.com`, `${'é'.repeat(32)}@x.com`],
	[max, max]
];
for (const [raw, want] of goodEmails) {
	ok('email accepted', parsed(signupSchema, signup({ email: raw })).data?.email === want, raw);
	ok('forgot email accepted', parsed(forgotSchema, { email: raw }).data?.email === want, raw);
}
for (const raw of [
	'',
	'plain',
	'a@b',
	'@b.com',
	'a@@b.com',
	'a@b@c.com',
	'a b@c.com',
	'a\u0000@c.com',
	'a@.b.com',
	'a@b.com.',
	'a@-b.com',
	'a@b-.com',
	'a@b..com',
	'a@b_c.com',
	'a@bü.com',
	`${'a'.repeat(65)}@x.com`,
	`${'é'.repeat(33)}@x.com`,
	`a${max}`
]) {
	ok(
		'email rejected',
		firstError(signupSchema, signup({ email: raw }), 'email') === 'invalid email address',
		raw
	);
	ok(
		'login email rejected',
		firstError(loginSchema, { email: raw, password: 'x' }, 'email') === 'invalid email address',
		raw
	);
}
ok(
	'login empty password rejected',
	firstError(loginSchema, { email: 'a@b.co', password: '' }, 'password') === 'enter your password',
	''
);

// Passwords (code points, White_Space).
const pwMsg = 'password must be 8-128 characters';
for (const p of ['12345678', '   a    ', 'é'.repeat(128), '\uFEFF'.repeat(8), '😀'.repeat(8)])
	ok('password accepted', parsed(signupSchema, signup({ password: p })).data !== null, p);
for (const [p, msg] of [
	['1234567', pwMsg],
	['x'.repeat(129), pwMsg],
	['😀'.repeat(4), pwMsg],
	['        ', 'password must not be only whitespace'],
	['\u0085'.repeat(8), 'password must not be only whitespace']
]) {
	ok('password rejected', firstError(signupSchema, signup({ password: p }), 'password') === msg, p);
	ok(
		'reset password rejected',
		firstError(resetSchema, { token: 'A'.repeat(43), password: p }, 'password') === msg,
		p
	);
	ok(
		'new password rejected',
		firstError(changePasswordSchema, { new_password: p }, 'new_password') === msg,
		p
	);
}
ok(
	'current password optional',
	parsed(changePasswordSchema, { new_password: pw }).data !== null,
	'no current'
);

// Tokens: canonical unpadded base64url of 32 bytes.
const goodToken = Buffer.from(new Uint8Array(32).fill(0xab)).toString('base64url');
for (const t of [goodToken, 'A'.repeat(43), `${'_'.repeat(42)}w`])
	ok('token accepted', parsed(verifySchema, { token: t }).data !== null, t);
for (const t of [
	'',
	'A'.repeat(42),
	'A'.repeat(44),
	`${'A'.repeat(42)}B`,
	`${goodToken}=`,
	`+${goodToken.slice(1)}`,
	`/${goodToken.slice(1)}`,
	`.${goodToken.slice(1)}`
])
	ok(
		'token rejected',
		firstError(verifySchema, { token: t }, 'token') === 'invalid or expired token',
		t
	);
for (let i = 0; i < 64; i++) {
	const t = `${'A'.repeat(42)}${'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_'[i]}`;
	const canonical = Buffer.from(Buffer.from(t, 'base64url')).toString('base64url') === t;
	ok(
		'token tail matches canonical decode',
		(parsed(verifySchema, { token: t }).data !== null) === canonical,
		t
	);
}

// URLs and slugs.
const maxUrl = `https://x.com/${'a'.repeat(2048 - 14)}`;
for (const u of ['HTTP://Example.com/path?q=1', 'https://notevil.com', maxUrl])
	ok('url accepted', parsed(createLinkSchema, { url: u }).data !== null, u);
const noScheme = 'url must start with http:// or https://';
for (const [u, msg] of [
	['javascript:alert(1)', noScheme],
	['data:text/html,x', noScheme],
	['localhost:8080/x', noScheme],
	['example.com:80/a', noScheme],
	['ftp://x.com', 'url must be http or https'],
	['http://user@x.com', 'url must not contain credentials'],
	['not a url', 'url must start with http:// or https://'],
	['example.com/abc', 'url must start with http:// or https://'],
	['www.example.com', 'url must start with http:// or https://'],
	['http://', 'invalid url'],
	[`${maxUrl}a`, 'url is too long']
])
	ok('url rejected', firstError(createLinkSchema, { url: u }, 'url') === msg, u);
const slugMsg = "slug must be 3-64 characters of letters, digits, '-' or '_'";
ok(
	'form url trimmed',
	parsed(createLinkFormSchema, { url: '  https://x.com  ', slug: '', permanent: false }).data
		?.url === 'https://x.com',
	'trim'
);
ok(
	'form url needs scheme',
	firstError(createLinkFormSchema, { url: 'example.com', slug: '', permanent: false }, 'url') ===
		'url must start with http:// or https://',
	'example.com'
);
const o = 't.test';
for (const [raw, want] of [
	['abc', 'abc'],
	['https://t.test/abc', 'abc'],
	['http://t.test/abc', 'abc'],
	['t.test/abc', 'abc'],
	['T.TEST/abc', 'abc'],
	['HTTPS://T.TEST/abc', 'abc'],
	['https://t.test/abc/', 'abc'],
	['t.test/a/b', null],
	['https://t.test/abc?x=1', 'abc'],
	['t.test/abc#f', 'abc'],
	['https://t.test/abc?', 'abc'],
	['http://t.test/abc#', 'abc'],
	['t.test/abc/?x=1', 'abc'],
	['https://t.test/abc#frag', 'abc'],
	['t.test/a/b?x', null],
	['t.test/?x', null],
	['abc?x', 'abc?x'],
	['https://t.test/', null],
	['t.test/', null],
	['https://other.test/abc', null],
	['t.test:8080/abc', null],
	['ftp://t.test/abc', null]
] as const)
	ok('slugFromInput', slugFromInput(raw, o) === want, raw);
ok('slugFromInput port host', slugFromInput('t.test:8080/abc', 't.test:8080') === 'abc', 'port');
ok(
	'form slug from link',
	parsed(createLinkFormSchema, {
		url: 'https://x.com',
		slug: 'https://t.test/abc',
		permanent: false
	}).data?.slug === 'abc',
	'link'
);
ok(
	'form slug blank',
	parsed(createLinkFormSchema, { url: 'https://x.com', slug: '', permanent: false }).data?.slug ===
		undefined,
	'blank'
);
ok(
	'form slug other link',
	firstError(
		createLinkFormSchema,
		{ url: 'https://x.com', slug: 'https://o.test/abc', permanent: false },
		'slug'
	) === 'enter a slug or a link on this site',
	'other'
);
ok(
	'update form slug from link',
	parsed(updateLinkFormSchema, { slug: 'https://t.test/abc' }).data?.slug === 'abc',
	'update'
);
ok('update form no slug', parsed(updateLinkFormSchema, {}).data !== null, 'none');
for (const s of ['abc', 'My-Link_1', 'a'.repeat(64)])
	ok('slug accepted', parsed(createLinkSchema, { url: 'https://x.com', slug: s }).data !== null, s);
for (const [s, msg] of [
	['ab', slugMsg],
	['a'.repeat(65), slugMsg],
	['a b', slugMsg],
	['é', slugMsg],
	['a.b', slugMsg],
	['api', 'slug is reserved'],
	['dashboard', 'slug is reserved'],
	['healthz', 'slug is reserved'],
	['report', 'slug is reserved']
]) {
	ok(
		'slug rejected',
		firstError(createLinkSchema, { url: 'https://x.com', slug: s }, 'slug') === msg,
		s
	);
	ok('update slug rejected', firstError(updateLinkSchema, { slug: s }, 'slug') === msg, s);
}
ok('update link empty ok', parsed(updateLinkSchema, {}).data !== null, {});

// Admin patch: typed enum and booleans.
ok(
	'admin patch ok',
	parsed(adminUserPatchSchema, { role: 'admin', disabled: false, email_verified: null }).data !==
		null,
	'ok'
);
ok(
	'admin role enum',
	parsed(adminUserPatchSchema, { role: 'owner' }).errors?.role !== undefined,
	'owner'
);
ok(
	'admin bool typed',
	parsed(adminUserPatchSchema, { email_verified: 'yes' }).errors?.email_verified !== undefined,
	'yes'
);

// Unknown fields rejected everywhere (server: deny_unknown_fields).
for (const [s, v] of [
	[signupSchema, signup({ role: 'admin' })],
	[loginSchema, { email: 'a@b.co', password: pw, x: 1 }],
	[forgotSchema, { email: 'a@b.co', x: 1 }],
	[verifySchema, { token: 'A'.repeat(43), x: 1 }],
	[updateNameSchema, { name: 'Ann', email: 'b@x.com' }],
	[createLinkSchema, { url: 'https://x.com', user_id: 1 }],
	[adminUserPatchSchema, { admin: true }]
] as [Schema, unknown][])
	ok('unknown field rejected', !s.safeParse(v).success, v);

// Reports: reason enum, details <= 2000 code points (trimmed, blank = none), optional email.
const rep = (over: Record<string, unknown>) => ({ slug: 'abc', reason: 'spam', ...over });
ok('report ok', parsed(reportSchema, rep({})).data !== null, 'ok');
ok(
	'report bad reason',
	firstError(reportSchema, rep({ reason: 'x' }), 'reason') ===
		'reason must be one of phishing, malware, spam, illegal, other',
	'x'
);
ok(
	'report details 2000',
	parsed(reportSchema, rep({ details: '𝒜'.repeat(2000) })).data !== null,
	2000
);
ok(
	'report details 2001',
	firstError(reportSchema, rep({ details: 'a'.repeat(2001) }), 'details') ===
		'details must be at most 2000 characters',
	2001
);
ok(
	'report blank details',
	parsed(reportSchema, rep({ details: '  ' })).data?.details === undefined,
	'blank'
);
ok(
	'report blank email',
	parsed(reportSchema, rep({ contact_email: ' ' })).data?.contact_email === undefined,
	'blank'
);
ok(
	'report email normalised',
	parsed(reportSchema, rep({ contact_email: ' A@B.co ' })).data?.contact_email === 'a@b.co',
	'norm'
);
ok(
	'report bad email',
	firstError(reportSchema, rep({ contact_email: 'nope' }), 'contact_email') ===
		'contact email is invalid',
	'nope'
);
ok('report unknown field', !reportSchema.safeParse(rep({ x: 1 })).success, 'x');
ok(
	'report form slug from link',
	parsed(reportFormSchema, rep({ slug: 'https://t.test/abc?x=1' })).data?.slug === 'abc',
	'link'
);
ok(
	'report form slug blank',
	firstError(reportFormSchema, rep({ slug: '' }), 'slug') === 'enter the short link',
	'blank'
);
ok(
	'report form slug other link',
	firstError(reportFormSchema, rep({ slug: 'o.test/abc' }), 'slug') ===
		'enter a slug or a link on this site',
	'other'
);
ok(
	'report form no reason',
	firstError(reportFormSchema, rep({ reason: undefined }), 'reason') === 'choose a reason',
	'none'
);

// Search query: White_Space trim, ≤200 code points.
ok('query trimmed', searchQuerySchema.safeParse('  findme\u0085').data === 'findme', 'findme');
ok('query blank', searchQuerySchema.safeParse('   ').data === undefined, 'blank');
ok('query 200', searchQuerySchema.safeParse('é'.repeat(200)).success, 200);
ok('query 201', !searchQuerySchema.safeParse('q'.repeat(201)).success, 201);

// Password-setting forms: `confirm` must repeat the new password.
ok(
	'signup confirm match',
	parsed(signupFormSchema, signup({ confirm: pw })).data !== null,
	'match'
);
ok(
	'signup confirm mismatch',
	firstError(signupFormSchema, signup({ confirm: pw + 'x' }), 'confirm') ===
		"the passwords don't match",
	'mismatch'
);
ok(
	'reset confirm mismatch',
	firstError(resetFormSchema, { token: 'A'.repeat(43), password: pw, confirm: '' }, 'confirm') !==
		null,
	'empty'
);
ok(
	'change confirm match',
	parsed(changePasswordFormSchema, { new_password: pw, confirm: pw }).data !== null,
	'match'
);
ok(
	'change confirm mismatch',
	firstError(
		changePasswordFormSchema,
		{ current_password: pw, new_password: pw, confirm: 'other' },
		'confirm'
	) !== null,
	'other'
);

console.log(`schemas: ${cases} cases ok`);
