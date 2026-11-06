<script lang="ts">
	import { shortToday } from '#lib/format.ts';
	import { invalidate } from '$app/navigation';
	import { api } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import NewPasswordFields from '#lib/components/NewPasswordFields.svelte';
	import Envelope from '#lib/components/Envelope.svelte';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import ProviderButtons from '#lib/components/ProviderButtons.svelte';
	import { spaForm } from '#lib/forms.ts';
	import { withNext } from '#lib/safe-next.ts';
	import { emailMax, signupFormSchema } from '#lib/schemas.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const form = spaForm(
		signupFormSchema,
		{ name: '', email: '', password: '', confirm: '' },
		async ({ confirm: _, ...body }) => {
			await api('POST', '/auth/signup', body);
			// The page load sees the new session and redirects to `next`.
			await invalidate('app:auth');
		}
	);
	const { form: formData, enhance, message, submitting } = form;
	const valid = $derived(signupFormSchema.safeParse($formData).success);
	const today = shortToday();
</script>

<svelte:head><title>Create account · tiiny</title></svelte:head>

{#snippet senderLabel(full: boolean)}
	<Envelope
		{full}
		from={[
			{ text: $formData.name, placeholder: 'Your name', strong: true },
			{ text: $formData.email, placeholder: 'you@example.com' }
		]}
		{valid}
		mark={{ label: 'Welcome', value: today }}
		facts={[
			'Free, and open source under the MIT licence.',
			'Clicks are counted; the people who make them aren’t tracked.',
			'Name every link yourself, or let tiiny pick one.'
		]}
	/>
{/snippet}

<AuthShell
	title="Create an account"
	intro={data.providers.email_verification
		? "We'll email you a link to verify your address, then you can start shortening."
		: 'Start shortening links as soon as you sign up.'}
	label={data.providers.signup && data.providers.password ? senderLabel : undefined}
>
	{#if !data.providers.signup}
		<p class="text-muted-foreground">
			This server isn't accepting new accounts. Ask an admin of this server for access.
		</p>
	{:else}
		<ProviderButtons providers={data.providers} verb="Sign up" next={data.next} />
		{#if data.providers.password}
			<form method="POST" class="grid gap-5" use:enhance>
				<Form.Field {form} name="name">
					<Form.Control>
						{#snippet children({ props })}
							<Form.Label>Name</Form.Label>
							<Input {...props} autocomplete="name" required bind:value={$formData.name} />
						{/snippet}
					</Form.Control>
					<Form.FieldErrors />
				</Form.Field>
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
				<NewPasswordFields {form} name="password" />
				<FormAlert message={$message} />
				<Form.Button disabled={$submitting}
					>{$submitting ? 'Creating account…' : 'Create account'}</Form.Button
				>
			</form>
		{/if}
	{/if}

	{#snippet footer()}
		Already have an account? <a class="text-link" href={withNext('/login', data.next)}>Sign in</a>
	{/snippet}
</AuthShell>
