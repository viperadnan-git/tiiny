<script lang="ts">
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, errorMessage } from '#lib/api-client.ts';
	import { applyUserAction, deleteUser, userActions, type UserAction } from '../admin-users.ts';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import * as Breadcrumb from '#lib/components/ui/breadcrumb/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import BrandIcon from '#lib/components/BrandIcon.svelte';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import LinkList from '#lib/components/LinkList.svelte';
	import UserAvatar from '#lib/components/UserAvatar.svelte';
	import UserBadges from '../UserBadges.svelte';
	import { formatCompact, formatNumber } from '#lib/format.ts';
	import { placeholderUserDetail } from '#lib/placeholders.ts';
	import type { AdminUserDetail } from '#lib/types.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const id = $derived(page.params.id);
	const email = $derived(page.url.searchParams.get('email'));

	let user = $state<AdminUserDetail | null>(null);
	let error = $state('');
	let busy = $state(false);
	$effect(() => {
		const want = id;
		user = null;
		error = '';
		api<AdminUserDetail>('GET', `/admin/users/${want}`).then(
			(u) => want === id && (user = u),
			(e) => want === id && (error = errorMessage(e))
		);
	});

	const loading = $derived(!user && !error);
	// From the route, so the skeleton already has the "(you)" label and your own (shorter) action set.
	const self = $derived(Number(id) === data.me.id);
	// Known before the load: your own account, or the email from the list link. The rest stands in.
	const known = $derived(self ? data.me.email : email);
	const u = $derived(
		user ?? {
			...placeholderUserDetail,
			...(self ? data.me : {}),
			email: known ?? placeholderUserDetail.email
		}
	);
	const label = $derived(user?.email ?? known ?? `User ${id}`);
	// Opened without ?email= (a reload, a pasted link): the address isn't known until the user loads.
	const labelLoading = $derived(!user?.email && !known && !error);
	const shown = $derived(labelLoading ? placeholderUserDetail.email : label);
	const signIns = $derived([...(u.has_password ? ['Password'] : []), ...u.providers]);

	async function run(a: UserAction) {
		if (!user) return;
		busy = true;
		const updated = await applyUserAction(user, a);
		if (updated && user) user = { ...user, ...updated };
		busy = false;
	}

	async function remove() {
		if (!user) return;
		busy = true;
		if (await deleteUser(user)) await goto('/admin/users');
		busy = false;
	}
</script>

<svelte:head><title>{label} · Admin · tiiny</title></svelte:head>

<div class="grid gap-6">
	<Breadcrumb.Root>
		<Breadcrumb.List>
			<Breadcrumb.Item><Breadcrumb.Link href="/admin/users">Users</Breadcrumb.Link></Breadcrumb.Item
			>
			<Breadcrumb.Separator />
			<Breadcrumb.Item
				><Breadcrumb.Page class="wrap-anywhere"
					><Skeleton loading={labelLoading} class="inline-block">{shown}</Skeleton></Breadcrumb.Page
				></Breadcrumb.Item
			>
		</Breadcrumb.List>
	</Breadcrumb.Root>

	<header class="grid gap-5 border-b pb-5 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
		<div class="flex min-w-0 items-start gap-4 sm:gap-5">
			<UserAvatar
				email={u.email}
				name={u.name}
				class="size-12 shrink-0 sm:size-16 {u.disabled ? 'opacity-60' : ''}"
			/>
			<div class="min-w-0">
				<h1 class="text-[clamp(24px,3.4vw,36px)] wrap-anywhere">
					<Skeleton loading={labelLoading} class="inline-block">{shown}</Skeleton>{#if self}<span
							class="ml-2 font-sans text-base text-muted-foreground">(you)</span
						>{/if}
				</h1>
				<p class="mt-1 flex flex-wrap gap-x-3 text-sm text-muted-foreground">
					{#if u.name}<span><Skeleton {loading} class="inline-block">{u.name}</Skeleton></span>{/if}
					<span class="font-mono text-[13px]"
						><Skeleton {loading} class="inline-block"
							>User #{id}, joined <RelativeTime unix={u.created_at} /></Skeleton
						></span
					>
				</p>
				<UserBadges {u} {loading} />
			</div>
		</div>
		<div class="flex flex-wrap gap-2 md:justify-end" role="group" aria-label="Actions for {label}">
			{#each userActions(u, self) as a (a.label)}
				<Skeleton {loading} class="rounded-full">
					<Button
						size="sm"
						variant={a.changes.disabled === false ? 'default' : 'secondary'}
						disabled={busy || loading}
						onclick={() => run(a)}>{a.label}</Button
					>
				</Skeleton>
			{/each}
			{#if !self}
				<Skeleton {loading} class="rounded-full">
					<Button
						size="sm"
						variant="destructive-secondary"
						disabled={busy || loading}
						onclick={remove}>Delete user</Button
					>
				</Skeleton>
			{/if}
		</div>
	</header>

	<FormAlert message={error} />
	{#if user?.disabled}
		<Alert.Root variant="destructive">
			<Alert.Title>This account is disabled</Alert.Title>
			<Alert.Description
				>They can't sign in and none of their links redirect. Enable them to restore both.</Alert.Description
			>
		</Alert.Root>
	{/if}

	<!-- One slip, four fields: dashed rules between them, like the shorten form. -->
	<Card.Root class="gap-0 py-0" aria-busy={loading}>
		<dl class="grid grid-cols-2 md:grid-cols-4">
			<div
				class="border-r-2 border-b-2 border-dashed border-border px-4 py-4 sm:px-6 md:border-b-0"
			>
				<dt class="text-[13px] text-muted-foreground">Links</dt>
				<dd class="mt-1 font-serif text-[28px] leading-none font-medium">
					<Skeleton {loading} class="inline-block">{formatNumber(u.link_count)}</Skeleton>
				</dd>
			</div>
			<div
				class="border-b-2 border-dashed border-border px-4 py-4 sm:px-6 md:border-r-2 md:border-b-0"
			>
				<dt class="text-[13px] text-muted-foreground">Clicks</dt>
				<dd
					class="mt-1 font-serif text-[28px] leading-none font-medium"
					title={formatNumber(u.click_count)}
				>
					<Skeleton {loading} class="inline-block">{formatCompact(u.click_count)}</Skeleton>
				</dd>
			</div>
			<div class="border-r-2 border-dashed border-border px-4 py-4 sm:px-6">
				<dt class="text-[13px] text-muted-foreground">Signs in with</dt>
				<dd class="mt-1.5 flex flex-wrap gap-1.5">
					{#each signIns.length ? signIns : ['none'] as m (m)}
						<Skeleton {loading} class="rounded-full">
							<Badge
								variant="outline"
								class="h-6 gap-1.5 px-2.5 text-[13px] font-normal capitalize"
							>
								{#if m === 'github' || m === 'google'}<BrandIcon
										name={m}
										class="size-3.5"
									/>{/if}{m === 'github' ? 'GitHub' : m}
							</Badge>
						</Skeleton>
					{/each}
				</dd>
			</div>
			<div class="px-4 py-4 sm:px-6">
				<dt class="text-[13px] text-muted-foreground">Email</dt>
				<dd
					class="mt-1.5 text-[15px] font-medium {u.email_verified ? '' : 'text-muted-foreground'}"
				>
					<Skeleton {loading} class="inline-block"
						>{u.email_verified ? 'Verified' : 'Not verified'}</Skeleton
					>
				</dd>
			</div>
		</dl>
	</Card.Root>

	{#key id}
		<LinkList endpoint="/admin/links" params={{ user_id: id }} title="Links">
			{#snippet empty()}
				<Empty.Root class="border-border">
					<Empty.Header>
						<Empty.Title>No links yet</Empty.Title>
						<Empty.Description>This user hasn't made any short links.</Empty.Description>
					</Empty.Header>
				</Empty.Root>
			{/snippet}
		</LinkList>
	{/key}
</div>
