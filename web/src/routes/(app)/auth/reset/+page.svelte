<script lang="ts">
	import { shortToday } from '#lib/format.ts';
	import { invalidate } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '#lib/api-client.ts';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Form from '#lib/components/ui/form/index.js';
	import NewPasswordFields from '#lib/components/NewPasswordFields.svelte';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import Envelope from '#lib/components/Envelope.svelte';
	import { spaForm } from '#lib/forms.ts';
	import { passwordMin, resetFormSchema, verifySchema } from '#lib/schemas.ts';

	const token = page.url.searchParams.get('token') ?? '';
	const tokenOk = verifySchema.safeParse({ token }).success;
	let done = $state(false);
	// The server's only 400 here: the token is unknown, used or expired.
	let spent = $state(false);

	const form = spaForm(
		resetFormSchema,
		{ token, password: '', confirm: '' },
		async ({ token, password }) => {
			try {
				await api('POST', '/auth/reset-password', { token, password });
			} catch (e) {
				if (e instanceof ApiError && e.status === 400) {
					spent = true;
					return;
				}
				throw e;
			}
			done = true;
			await invalidate('app:auth');
		}
	);
	const { form: formData, enhance, message, submitting } = form;
	const valid = $derived(done || spent || resetFormSchema.safeParse($formData).success);
	const today = shortToday();
</script>

<svelte:head><title>Choose a new password · tiiny</title></svelte:head>

{#snippet sealed(full: boolean)}
	<Envelope
		{full}
		from={[
			{ text: 'New password', strong: true },
			{ text: '•'.repeat(Math.min($formData.password.length, 16)), placeholder: '••••••••' }
		]}
		to={[{ text: 'Your account', strong: true }, { text: 'Sign in again after this' }]}
		{valid}
		mark={spent ? { label: 'Return to', value: 'sender' } : { label: 'Sealed', value: today }}
		facts={[
			`Use at least ${passwordMin} characters. A few unrelated words are easy to remember and hard to guess.`,
			'Saving it signs you out on every device.',
			'Google or GitHub sign-in is disconnected. Reconnect it in Settings.'
		]}
	/>
{/snippet}

<AuthShell
	title={done ? 'Password updated' : spent ? 'This link can’t be used' : 'Choose a new password'}
	intro={done || spent || !tokenOk ? undefined : 'Pick something you don’t use anywhere else.'}
	label={tokenOk ? sealed : undefined}
>
	{#if !tokenOk}
		<Alert.Root variant="destructive"
			><Alert.Description
				>This reset link is incomplete or damaged. Open the link from your email again.</Alert.Description
			></Alert.Root
		>
	{:else if spent}
		<Alert.Root variant="destructive" role="alert">
			<Alert.Description>
				This reset link has expired or was already used. Your password hasn't changed. Each link
				works once, within an hour, and only the newest one works.
			</Alert.Description>
		</Alert.Root>
		<Button href="/auth/forgot" class="justify-self-start">Send a new link</Button>
	{:else if done}
		<div class="grid gap-2" role="status">
			<p>
				Sign in with your new password. You were signed out on every device, and your email is now
				verified.
			</p>
			<p class="text-muted-foreground">
				If you used Google or GitHub to sign in, reconnect it in Settings.
			</p>
		</div>
		<Button href="/login" class="justify-self-start">Sign in</Button>
	{:else}
		<form method="POST" class="grid gap-5" use:enhance>
			<NewPasswordFields {form} name="password" label="New password" />
			<FormAlert message={$message} />
			<Form.Button disabled={$submitting}
				>{$submitting ? 'Saving…' : 'Set new password'}</Form.Button
			>
		</form>
	{/if}
</AuthShell>
