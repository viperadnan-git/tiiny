<script lang="ts">
	import { api } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import Envelope from '#lib/components/Envelope.svelte';
	import { shortHost } from '#lib/origin.ts';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import { canResetPassword } from '#lib/format.ts';
	import { spaForm } from '#lib/forms.ts';
	import { emailMax, forgotSchema } from '#lib/schemas.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let sentTo = $state<string | null>(null);

	const form = spaForm(
		forgotSchema,
		{ email: '' },
		async (body) => {
			await api('POST', '/auth/forgot-password', body);
			sentTo = body.email;
		},
		{ resetForm: false }
	);
	const { form: formData, enhance, message, submitting } = form;
	const valid = $derived(sentTo !== null || forgotSchema.safeParse($formData).success);
</script>

<svelte:head><title>Reset password · tiiny</title></svelte:head>

{#snippet request(full: boolean)}
	<Envelope
		{full}
		from={[{ text: sentTo ?? $formData.email, placeholder: 'you@example.com' }]}
		to={[{ text: 'Password reset', strong: true }, { text: shortHost }]}
		{valid}
		mark={{ label: sentTo ? 'Posted' : 'Valid', value: sentTo ? 'today' : '1 hr' }}
		facts={[
			'The link works once, within an hour.',
			'Your current password keeps working until you choose a new one.',
			'Asked more than once? Only the newest link works.'
		]}
	/>
{/snippet}

<AuthShell
	title={sentTo ? 'Check your email' : 'Reset your password'}
	intro={sentTo
		? undefined
		: "Enter the email you sign in with and we'll send a link to choose a new password."}
	label={canResetPassword(data.providers) ? request : undefined}
>
	{#if !canResetPassword(data.providers)}
		<p class="text-muted-foreground">
			This server can't send email, so an admin has to reset your password. They can run
			<code class="font-mono text-foreground">tiiny reset-password &lt;email&gt;</code> on the server.
		</p>
	{:else if sentTo}
		<p role="status">
			If an account exists for <strong class="font-mono font-bold break-all">{sentTo}</strong>, a
			reset link is on its way. If it hasn't arrived in a few minutes, check your spam folder.
		</p>
		<Button variant="secondary" class="justify-self-start" onclick={() => (sentTo = null)}
			>Use a different email</Button
		>
	{:else}
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
			<FormAlert message={$message} />
			<Form.Button disabled={$submitting}
				>{$submitting ? 'Sending…' : 'Send reset link'}</Form.Button
			>
		</form>
	{/if}

	{#snippet footer()}
		<a class="text-link" href="/login">Back to sign in</a>
	{/snippet}
</AuthShell>
