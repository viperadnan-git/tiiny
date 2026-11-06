<script lang="ts">
	import { untrack } from 'svelte';
	import { goto, invalidate } from '$app/navigation';
	import { page } from '$app/state';
	import { toast } from 'svelte-sonner';
	import { api, errorMessage } from '#lib/api-client.ts';
	import { toastError } from '#lib/toast.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Item from '#lib/components/ui/item/index.js';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import BrandIcon from '#lib/components/BrandIcon.svelte';
	import NewPasswordFields from '#lib/components/NewPasswordFields.svelte';
	import PasswordInput from '#lib/components/PasswordInput.svelte';
	import SettingsSection from './SettingsSection.svelte';
	import UserAvatar from '#lib/components/UserAvatar.svelte';
	import { confirm } from '#lib/confirm.svelte.ts';
	import { spaForm } from '#lib/forms.ts';
	import { enabledProviders, oauthErrorMessage, providerLabel } from '#lib/format.ts';
	import { changePasswordFormSchema, updateNameSchema } from '#lib/schemas.ts';
	import type { Account, OauthProvider, User } from '#lib/types.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const linkError = oauthErrorMessage(page.url.searchParams.get('error'));
	const hasPassword = $derived(data.me.has_password);

	let accounts = $state<Account[] | null>(null);
	let accountsError = $state<string | null>(null);
	const enabled = $derived(enabledProviders(data.providers));
	let busy = $state<string | null>(null);

	$effect(() => {
		if (!enabled.length) return;
		api<Account[]>('GET', '/auth/accounts')
			.then((a) => (accounts = a))
			.catch((e) => (accountsError = errorMessage(e)));
	});

	const nameForm = spaForm(
		updateNameSchema,
		{ name: untrack(() => data.me.name) ?? '' },
		async (body) => {
			const me = await api<User>('PATCH', '/auth/me', body);
			nameData.set({ name: me.name ?? '' });
			toast.success('Name saved');
			await invalidate('app:auth');
		},
		{ resetForm: false }
	);
	const {
		form: nameData,
		enhance: nameEnhance,
		message: nameMessage,
		submitting: nameSubmitting
	} = nameForm;

	const pwForm = spaForm(
		changePasswordFormSchema,
		{ current_password: '', new_password: '', confirm: '' },
		async ({ current_password, new_password }) => {
			await api(
				'PUT',
				'/auth/password',
				hasPassword ? { current_password, new_password } : { new_password }
			);
			if (hasPassword)
				toast.success('Password changed', { description: 'Other devices were signed out.' });
			else
				toast.success('Password set', {
					description: 'You can now sign in with your email and password.'
				});
			await invalidate('app:auth');
		}
	);
	const { form: pwData, enhance: pwEnhance, message: pwMessage, submitting: pwSubmitting } = pwForm;

	async function disconnect(p: OauthProvider) {
		const ok = await confirm({
			title: `Disconnect ${providerLabel[p]}?`,
			description: `You won't be able to sign in with ${providerLabel[p]} until you connect it again.`,
			action: `Disconnect ${providerLabel[p]}`,
			cancel: 'Keep connected'
		});
		if (!ok) return;
		busy = p;
		accountsError = null;
		try {
			await api('DELETE', `/auth/accounts/${p}`);
			accounts = (accounts ?? []).filter((a) => a.provider !== p);
			toast.success(`${providerLabel[p]} disconnected`);
		} catch (err) {
			accountsError = errorMessage(err);
		} finally {
			busy = null;
		}
	}

	async function deleteAccount() {
		const ok = await confirm({
			title: 'Delete your account?',
			description: `${data.me.email} and all its links and stats are deleted, and permanent or blocked slugs are retired until an admin releases them. This can't be undone.`,
			action: 'Delete account',
			cancel: 'Keep account',
			destructive: true,
			typeToConfirm: data.me.email
		});
		if (!ok) return;
		busy = 'delete';
		try {
			await api('DELETE', '/auth/me');
			await invalidate('app:auth');
			toast.success('Account deleted');
			await goto('/');
		} catch (err) {
			toastError("Couldn't delete your account", err);
			busy = null;
		}
	}
</script>

<svelte:head><title>Settings · tiiny</title></svelte:head>

<div class="mx-auto max-w-230">
	<header class="flex flex-wrap items-center gap-x-5 gap-y-4 pb-8 sm:pb-10">
		<UserAvatar email={data.me.email} name={data.me.name} class="size-16 sm:size-20" />
		<div class="min-w-0 flex-1">
			<h1 class="truncate text-[clamp(32px,4.4vw,48px)]">{data.me.name || 'Settings'}</h1>
			<p class="mt-1 flex flex-wrap items-center gap-2 font-mono text-[15px] text-muted-foreground">
				<span class="min-w-0 truncate">{data.me.email}</span>
				{#if data.me.email_verified}<Badge variant="stamp">verified</Badge>{/if}
				{#if data.me.role === 'admin'}<Badge variant="stamp">admin</Badge>{/if}
			</p>
		</div>
	</header>

	<SettingsSection
		id="profile-heading"
		title="Profile"
		description="Shown in the menu and to admins of this server."
	>
		<form
			method="POST"
			class="grid max-w-md gap-4"
			use:nameEnhance
			aria-labelledby="profile-heading"
		>
			<Form.Field form={nameForm} name="name">
				<Form.Control>
					{#snippet children({ props })}
						<Form.Label>Name</Form.Label>
						<Input {...props} autocomplete="name" required bind:value={$nameData.name} />
					{/snippet}
				</Form.Control>
				{#if !data.me.name}<Form.Description
						>Add your name to finish setting up your account.</Form.Description
					>{/if}
				<Form.FieldErrors />
			</Form.Field>
			<FormAlert message={$nameMessage} />
			<div><Form.Button variant="secondary" disabled={$nameSubmitting}>Save name</Form.Button></div>
		</form>
	</SettingsSection>

	{#if data.providers.password}
		<SettingsSection
			id="password-heading"
			title={hasPassword ? 'Password' : 'Set a password'}
			description={hasPassword
				? 'Changing it signs out your other devices.'
				: 'Add a password to sign in with your email as well.'}
		>
			<form
				method="POST"
				class="grid max-w-md gap-5"
				use:pwEnhance
				aria-labelledby="password-heading"
			>
				<input
					type="email"
					class="hidden"
					autocomplete="username"
					value={data.me.email}
					aria-hidden="true"
					tabindex="-1"
				/>
				{#if hasPassword}
					<Form.Field form={pwForm} name="current_password">
						<Form.Control>
							{#snippet children({ props })}
								<Form.Label>Current password</Form.Label>
								<PasswordInput
									{...props}
									autocomplete="current-password"
									required
									bind:value={$pwData.current_password}
								/>
							{/snippet}
						</Form.Control>
						<Form.FieldErrors />
					</Form.Field>
				{/if}
				<NewPasswordFields form={pwForm} name="new_password" label="New password" />
				<FormAlert message={$pwMessage} />
				<div>
					<Form.Button disabled={$pwSubmitting}
						>{hasPassword ? 'Change password' : 'Set password'}</Form.Button
					>
				</div>
			</form>
		</SettingsSection>
	{/if}

	{#if enabled.length}
		<SettingsSection
			id="accounts-heading"
			title="Connected accounts"
			description="Sign in with these instead of a password."
		>
			<div class="grid gap-4">
				<FormAlert message={linkError} />
				<FormAlert message={accountsError} />
				<!-- Unknown after a failed load: no rows rather than a guessed "Not connected" or an endless skeleton. -->
				{#if accounts || !accountsError}<Item.Group class="gap-2.5">
						{#each enabled as p (p)}
							{@const connected = accounts?.some((a) => a.provider === p) ?? false}
							<Item.Root variant="outline" class="flex-wrap">
								<Item.Media class="size-10 rounded-full border bg-background"
									><BrandIcon name={p} /></Item.Media
								>
								<Item.Content>
									<Item.Title>{providerLabel[p]}</Item.Title>
									<Item.Description class={connected ? 'text-stamp' : ''}>
										<Skeleton loading={accounts === null} class="inline-block"
											>{connected ? 'Connected' : 'Not connected'}</Skeleton
										>
									</Item.Description>
								</Item.Content>
								<Item.Actions class="basis-full sm:basis-auto">
									{#if connected}
										<Button
											variant="secondary"
											size="sm"
											onclick={() => disconnect(p)}
											disabled={busy === p}>Disconnect</Button
										>
									{:else if data.me.email_verified}
										<Skeleton loading={accounts === null} class="rounded-full">
											<Button
												variant="secondary"
												size="sm"
												href="/api/v1/auth/oauth/{p}/start?link=1"
												data-sveltekit-reload>Connect</Button
											>
										</Skeleton>
									{:else}
										<span class="text-sm text-muted-foreground">
											<Skeleton loading={accounts === null} class="inline-block">
												{data.providers.email_verification
													? 'Verify your email to connect an account'
													: 'An admin has to verify your email before you can connect an account'}
											</Skeleton>
										</span>
									{/if}
								</Item.Actions>
							</Item.Root>
						{/each}
					</Item.Group>{/if}
			</div>
		</SettingsSection>
	{/if}

	<SettingsSection
		id="delete-heading"
		title="Delete account"
		description="Deletes your account, every link you made and all their stats."
	>
		<Item.Root
			variant="outline"
			class="justify-between gap-4 rounded-2xl border-destructive/40 bg-destructive/5 px-4 py-4 sm:px-5"
		>
			<Item.Content class="max-w-[44ch] flex-[0_1_auto]"
				><Item.Description class="line-clamp-none text-[15px] leading-[1.6] text-foreground"
					>Permanent slugs are retired until an admin releases them. This can't be undone.</Item.Description
				></Item.Content
			>
			<Item.Actions
				><Button
					variant="destructive-secondary"
					onclick={deleteAccount}
					disabled={busy === 'delete'}>Delete account</Button
				></Item.Actions
			>
		</Item.Root>
	</SettingsSection>
</div>
