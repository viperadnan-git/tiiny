<script lang="ts">
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { goto } from '$app/navigation';
	import { api, qs } from '#lib/api-client.ts';
	import { applyUserAction, deleteUser, userActions, type UserAction } from './admin-users.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Item from '#lib/components/ui/item/index.js';
	import ListHeader from '#lib/components/ListHeader.svelte';
	import ListStatus from '#lib/components/ListStatus.svelte';
	import { placeholderUser, skeletonRows } from '#lib/placeholders.ts';
	import UserAvatar from '#lib/components/UserAvatar.svelte';
	import UserBadges from './UserBadges.svelte';
	import { paged } from '#lib/paged.svelte.ts';
	import { formatNumber } from '#lib/format.ts';
	import type { AdminUserRow, UserPage } from '#lib/types.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	let busy = $state<number | null>(null);
	const list = paged<AdminUserRow>(async (query, cursor) => {
		const page = await api<UserPage>('GET', `/admin/users${qs({ query, cursor })}`);
		return { items: page.users, next_cursor: page.next_cursor };
	});

	/** Marks `u` busy while `fn` runs. */
	async function act(u: AdminUserRow, fn: () => Promise<void>) {
		busy = u.id;
		try {
			await fn();
		} finally {
			busy = null;
		}
	}

	const run = (u: AdminUserRow, a: UserAction) =>
		act(u, async () => {
			const updated = await applyUserAction(u, a);
			if (updated)
				list.items = list.items.map((x) =>
					x.id === u.id ? { ...updated, link_count: x.link_count } : x
				);
		});

	const remove = (u: AdminUserRow) =>
		act(u, async () => {
			if (await deleteUser(u)) list.items = list.items.filter((x) => x.id !== u.id);
		});

	const userHref = (u: AdminUserRow) => `/admin/users/${u.id}?email=${encodeURIComponent(u.email)}`;
</script>

<svelte:head><title>Users · Admin · tiiny</title></svelte:head>

<div class="grid gap-6">
	<ListHeader
		title="Users"
		search={{ id: 'user-search', label: 'Search users', placeholder: 'Search by email or name' }}
		bind:value={list.search}
	/>

	<FormAlert message={list.error} />

	{#if list.items.length || list.skeleton}
		{@const rows = list.rows(skeletonRows(placeholderUser))}
		<Card.Root class="gap-0 py-0" aria-busy={list.loading}>
			<ul>
				{#each rows as u, i (u.id)}
					{@const loading = u.id < 0}
					{@const self = u.id === data.me.id}
					<li class={i ? 'border-t' : ''}>
						<Item.Root
							class="flex-nowrap items-start gap-3 rounded-none px-4 py-4 sm:gap-4 sm:px-6 {u.disabled
								? 'opacity-60'
								: ''}"
						>
							<Item.Media class="mt-0.5"
								><Skeleton {loading} class="rounded-full"
									><UserAvatar email={u.email} name={u.name} class="size-10" /></Skeleton
								></Item.Media
							>
							<Item.Content class="min-w-0 gap-0">
								<p class="min-w-0 font-semibold wrap-anywhere">
									<a class="hover:text-stamp" href={loading ? undefined : userHref(u)}
										><Skeleton {loading} class="inline-block"
											>{@const at = u.email.lastIndexOf('@')}{u.email.slice(0, at)}<wbr
											/>{u.email.slice(at)}</Skeleton
										></a
									>
									{#if self}<span class="text-sm font-normal text-muted-foreground">
											(you)</span
										>{/if}
								</p>
								<p class="mt-0.5 flex flex-wrap gap-x-3 text-sm text-muted-foreground">
									{#if u.name}<span class="min-w-0 truncate"
											><Skeleton {loading} class="inline-block max-w-full">{u.name}</Skeleton></span
										>{/if}
									<span class="font-mono text-[13px]"
										><Skeleton {loading} class="inline-block"
											>Joined <RelativeTime unix={u.created_at} /></Skeleton
										></span
									>
								</p>
								<UserBadges {u} {loading} />
							</Item.Content>
							<Item.Actions class="shrink-0 items-start gap-1 sm:gap-3">
								<Skeleton {loading} class="rounded-md">
									<a
										class="grid min-w-12 justify-items-end leading-none hover:text-stamp"
										href={userHref(u)}
										aria-label="{formatNumber(u.link_count)} links by {u.email}"
									>
										<span class="font-serif text-[24px] font-medium"
											>{formatNumber(u.link_count)}</span
										>
										<span class="mt-1 text-xs text-muted-foreground"
											>{u.link_count === 1 ? 'link' : 'links'}</span
										>
									</a>
								</Skeleton>
								<Skeleton {loading} class="rounded-full"
									><DropdownMenu.Root>
										<DropdownMenu.Trigger disabled={busy === u.id || loading}>
											{#snippet child({ props })}
												<Button
													{...props}
													size="icon-sm"
													variant="ghost"
													aria-label="Actions for {u.email}"><EllipsisIcon /></Button
												>
											{/snippet}
										</DropdownMenu.Trigger>
										<DropdownMenu.Content align="end">
											<DropdownMenu.Item onSelect={() => goto(userHref(u))}
												>View links</DropdownMenu.Item
											>
											{#each userActions(u, self) as a (a.label)}
												<DropdownMenu.Item onSelect={() => run(u, a)}>{a.label}</DropdownMenu.Item>
											{/each}
											{#if !self}
												<DropdownMenu.Separator />
												<DropdownMenu.Item variant="destructive" onSelect={() => remove(u)}
													>Delete</DropdownMenu.Item
												>
											{/if}
										</DropdownMenu.Content>
									</DropdownMenu.Root></Skeleton
								>
							</Item.Actions>
						</Item.Root>
					</li>
				{/each}
			</ul>
		</Card.Root>
	{/if}

	<ListStatus {list} hint="Search looks at email addresses and names.">
		{#snippet empty()}
			<Empty.Root class="border-border">
				<Empty.Header>
					<Empty.Title>No users yet</Empty.Title>
					<Empty.Description>People who sign up appear here.</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{/snippet}
	</ListStatus>
</div>
