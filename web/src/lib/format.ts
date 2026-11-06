import type { Providers } from './types.ts';

const numFmt = new Intl.NumberFormat();
const compactFmt = new Intl.NumberFormat(undefined, {
	notation: 'compact',
	maximumFractionDigits: 1
});

export const formatNumber = (n: number) => numFmt.format(n);
export const formatCompact = (n: number) => compactFmt.format(n);

const oauthErrors: Record<string, string> = {
	state: 'That sign-in attempt expired or was started in another tab. Try again.',
	unauthorized: 'Sign in first, then connect the account from Settings.',
	denied: 'Sign-in was cancelled at the provider.',
	provider: "The provider didn't respond as expected. Try again in a moment.",
	email_unverified: "Your email at that provider isn't verified. Verify it there, then try again.",
	email_invalid:
		"Your email at that provider isn't a valid address. Change it there, then try again.",
	signup_disabled: "This server isn't accepting new accounts.",
	disabled: 'This account has been disabled. Contact an admin of this server.',
	already_linked: 'That provider account is already connected to a different tiiny account.',
	verify_email_first: 'Verify your email address before connecting another sign-in method.',
	internal: 'Something went wrong on the server. Try again.'
};

export const oauthErrorMessage = (code: string | null) =>
	code ? (oauthErrors[code] ?? 'Sign-in failed. Try again.') : null;

export const providerLabel = { google: 'Google', github: 'GitHub' } as const;

export const enabledProviders = (p: Providers) =>
	(['google', 'github'] as const).filter((k) => p[k]);

/** Resetting needs both a password to reset and email to send the link. */
export const canResetPassword = (p: Providers) => p.email_verification && p.password;

/** "6 Nov": the date stamped on auth envelopes. */
export const shortToday = () =>
	new Date().toLocaleDateString('en-GB', { day: 'numeric', month: 'short' });
