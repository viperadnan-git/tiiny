<script lang="ts">
	import { invalidate } from '$app/navigation';
	import { api } from '#lib/api-client.ts';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import PasswordInput from '#lib/components/PasswordInput.svelte';
	import Envelope from '#lib/components/Envelope.svelte';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import ProviderButtons from '#lib/components/ProviderButtons.svelte';
	import { spaForm } from '#lib/forms.ts';
	import {
		canResetPassword,
		enabledProviders,
		oauthErrorMessage,
		providerLabel
	} from '#lib/format.ts';
	import { withNext } from '#lib/safe-next.ts';
	import { emailMax, loginSchema } from '#lib/schemas.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const oauthError = $derived(oauthErrorMessage(data.error));
	const verifying = $derived(data.next?.startsWith('/auth/verify') ?? false);
	const canReset = $derived(canResetPassword(data.providers));
	const oauthNames = $derived(enabledProviders(data.providers).map((p) => providerLabel[p]));

	const form = spaForm(loginSchema, { email: '', password: '' }, async (body) => {
		await api('POST', '/auth/login', body);
		// The page load sees the new session and redirects to `next`.
		await invalidate('app:auth');
	});
	const { form: formData, enhance, message, submitting } = form;
	const valid = $derived(loginSchema.safeParse($formData).success);
</script>

<svelte:head><title>Sign in · tiiny</title></svelte:head>

{#snippet senderLabel(full: boolean)}
	<Envelope
		{full}
		from={[{ text: $formData.email, placeholder: 'you@example.com' }]}
		{valid}
		mark={{ label: 'Welcome', value: 'back' }}
		facts={[
			'Your links and their stats are where you left them.',
			'Paste a link anywhere on the dashboard to shorten it.',
			'You stay signed in on this device for 30 days.'
		]}
	/>
{/snippet}

<AuthShell
	title="Sign in"
	intro="Welcome back. Your links are where you left them."
	label={data.providers.password ? senderLabel : undefined}
>
	{#if verifying}
		<Alert.Root>
			<Alert.Description class="text-foreground">
				Sign in to the account you're verifying. Can't sign in?
				{#if canReset}<a class="text-link" href="/auth/forgot">Reset your password</a>{/if}
				{#if oauthNames.length}
					{canReset ? 'or' : ''} continue with
					{oauthNames.join(' or ')} above.
				{/if}
			</Alert.Description>
		</Alert.Root>
	{/if}
	{#if oauthError}
		<Alert.Root variant="destructive" role="alert"
			><Alert.Description>{oauthError}</Alert.Description></Alert.Root
		>
	{/if}

	<ProviderButtons providers={data.providers} next={data.next} />

	{#if data.providers.password}
		<form method="POST" class="grid gap-5" use:enhance>
			<Form.Field {form} name="email">
				<Form.Control>
					{#snippet children({ props })}
						<Form.Label>Email</Form.Label>
						<Input
							{...props}
							type="email"
							autocomplete="email"
							required
							maxlength={emailMax}
							bind:value={$formData.email}
						/>
					{/snippet}
				</Form.Control>
				<Form.FieldErrors />
			</Form.Field>
			<Form.Field {form} name="password">
				<Form.Control>
					{#snippet children({ props })}
						<div class="flex items-baseline justify-between">
							<Form.Label>Password</Form.Label>
							{#if data.providers.email_verification}
								<a class="text-link text-sm" href="/auth/forgot">Forgot password?</a>
							{/if}
						</div>
						<PasswordInput
							{...props}
							autocomplete="current-password"
							required
							bind:value={$formData.password}
						/>
					{/snippet}
				</Form.Control>
				<Form.FieldErrors />
			</Form.Field>
			<FormAlert message={$message} />
			<Form.Button disabled={$submitting}>{$submitting ? 'Signing in…' : 'Sign in'}</Form.Button>
		</form>
	{/if}

	{#snippet footer()}
		{#if data.providers.signup}
			New here? <a class="text-link" href={withNext('/signup', data.next)}>Create an account</a>
		{/if}
	{/snippet}
</AuthShell>
