<script lang="ts">
	import { goto, invalidate } from '$app/navigation';
	import { page } from '$app/state';
	import { mode, setMode, userPrefersMode } from 'mode-watcher';
	import FlagIcon from '@lucide/svelte/icons/flag';
	import LinkIcon from '@lucide/svelte/icons/link';
	import LogOutIcon from '@lucide/svelte/icons/log-out';
	import MonitorIcon from '@lucide/svelte/icons/monitor';
	import MoonIcon from '@lucide/svelte/icons/moon';
	import SearchIcon from '@lucide/svelte/icons/search';
	import SettingsIcon from '@lucide/svelte/icons/settings';
	import ShieldUserIcon from '@lucide/svelte/icons/shield-user';
	import SunIcon from '@lucide/svelte/icons/sun';
	import { api, qs } from '#lib/api-client.ts';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Command from '#lib/components/ui/command/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import * as Kbd from '#lib/components/ui/kbd/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { Spinner } from '#lib/components/ui/spinner/index.js';
	import Logo from '#lib/components/Logo.svelte';
	import UserAvatar from '#lib/components/UserAvatar.svelte';
	import { hideOnScroll } from '#lib/hide-on-scroll.ts';
	import { openReports, refreshOpenReports } from '#lib/report-count.svelte.ts';
	import { shortHost } from '#lib/origin.ts';
	import { slugFromInput } from '#lib/schemas.ts';
	import { formatNumber } from '#lib/format.ts';
	import { placeholderLink, placeholderUser, skeletonRows } from '#lib/placeholders.ts';
	import type { AdminUserRow, Link, LinkPage, Providers, User, UserPage } from '#lib/types.ts';

	let { me, providers }: { me: User | null; providers: Providers } = $props();

	$effect(() => {
		if (admin) void refreshOpenReports();
	});

	const mac = /Mac|iPhone|iPad/.test(navigator.platform);
	let commandOpen = $state(false);
	let search = $state('');
	let found = $state<{ links: Link[]; users: AdminUserRow[] } | null>(null);
	/** Highlighted item's value; bits-ui only moves it on typing, so results set it when they land. */
	let selected = $state('');
	const admin = $derived(me?.role === 'admin');
	let searching = $state(false);
	const q = $derived(search.trim().toLowerCase());

	const pages = $derived([
		{ label: 'Shorten a link', href: '/dashboard#new' },
		{ label: 'Your links', href: '/dashboard' },
		{ label: 'Settings', href: '/dashboard/settings' },
		...(admin
			? [
					{ label: 'Users', href: '/admin/users' },
					{ label: 'Abuse reports', href: '/admin/reports' }
				]
			: [])
	]);
	const actions = $derived([
		{
			value: 'do:theme',
			label: `Switch to ${mode.current === 'dark' ? 'light' : 'dark'} theme`,
			run: () => setMode(mode.current === 'dark' ? 'light' : 'dark')
		},
		{ value: 'do:signout', label: 'Sign out', run: signOut }
	]);
	// Skeletons only before the first results; refining keeps the last results, dimmed, until new ones land.
	const loading = $derived(searching && !found);
	const stale = $derived(searching && !!found);
	const linkRows = $derived(loading ? skeletonRows(placeholderLink) : (found?.links ?? []));
	const userRows = $derived(loading ? skeletonRows(placeholderUser) : (found?.users ?? []));
	// Same rule as the server's search: case-insensitive substring.
	const matches = (c: { label: string }) => c.label.toLowerCase().includes(q);
	const shownPages = $derived(pages.filter(matches));
	const shownActions = $derived(actions.filter(matches));

	async function signOut() {
		await api('POST', '/auth/logout').catch(() => {});
		await invalidate('app:auth');
		await goto('/');
	}

	const close = () => (commandOpen = false);

	function run(fn: () => unknown) {
		commandOpen = false;
		fn();
	}

	// Every way of closing (Esc, ⌘K, a pick, a click outside) starts the next search fresh.
	$effect(() => {
		if (!commandOpen) search = '';
	});

	$effect(() => {
		const term = search.trim();
		if (!commandOpen || !me || !term) {
			found = null;
			searching = false;
			return;
		}
		searching = true;
		let live = true;
		const t = setTimeout(() => {
			// A pasted short link searches its slug; a destination URL matches whatever its scheme or trailing slash.
			const linkTerm =
				slugFromInput(term, shortHost) ?? term.replace(/^https?:\/\//i, '').replace(/\/$/, '');
			const query = (t: string) => qs({ query: t.slice(0, 200), limit: 5 });
			// Admins search every link, not just their own.
			Promise.all([
				// Exact slug first, within the newest 5 matches only (ranking on the server would cover all).
				api<LinkPage>('GET', `${admin ? '/admin/links' : '/links'}${query(linkTerm)}`).then((p) =>
					p.links.sort((a, b) => Number(b.slug === linkTerm) - Number(a.slug === linkTerm))
				),
				admin ? api<UserPage>('GET', `/admin/users${query(term)}`).then((p) => p.users) : []
			])
				.then(([links, users]) => {
					if (!live) return;
					found = { links, users };
					const first = links[0] ? `link:${links[0].id}` : users[0] ? `user:${users[0].id}` : null;
					if (first) selected = first;
				})
				.catch(() => live && (found = { links: [], users: [] }))
				.finally(() => live && (searching = false));
		}, 200);
		return () => {
			live = false;
			clearTimeout(t);
		};
	});

	function onkeydown(e: KeyboardEvent) {
		if (me && e.key === 'k' && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			commandOpen = !commandOpen;
		}
	}
</script>

<svelte:window {onkeydown} />

<header use:hideOnScroll class="auto-hide sticky top-0 z-40 pt-2 sm:pt-3">
	<div class="wrap">
		<div
			class="flex h-14 items-center gap-2 rounded-full border bg-[color-mix(in_oklab,var(--sheet)_82%,transparent)] pr-1.5 pl-4 shadow-paper backdrop-blur-md sm:gap-6 sm:pl-6"
		>
			<Logo href={me ? '/dashboard' : '/'} class="text-[24px]" />
			{#if me}
				<div class="ml-auto flex items-center gap-1 sm:gap-2">
					<Button
						variant="secondary"
						size="sm"
						class="hidden gap-3 pr-2 font-normal text-muted-foreground md:inline-flex"
						aria-haspopup="dialog"
						aria-expanded={commandOpen}
						onclick={() => (commandOpen = true)}
					>
						<SearchIcon />Search or jump to…<Kbd.Root>{mac ? '⌘K' : 'Ctrl K'}</Kbd.Root>
					</Button>
					<Button
						variant="ghost"
						size="icon"
						class="md:hidden"
						aria-label="Search or jump to"
						aria-haspopup="dialog"
						aria-expanded={commandOpen}
						onclick={() => (commandOpen = true)}
					>
						<SearchIcon />
					</Button>
					<DropdownMenu.Root>
						<DropdownMenu.Trigger>
							{#snippet child({ props })}
								<Button
									{...props}
									variant="ghost"
									size="icon"
									class="relative hover:bg-transparent"
									aria-label={openReports.count
										? `Account menu, ${openReports.count} open reports`
										: 'Account menu'}
								>
									<UserAvatar email={me.email} name={me.name} class="size-9" />
									{#if openReports.count}<span
											class="absolute top-0.5 right-0.5 size-2.5 rounded-full bg-destructive ring-2 ring-card"
										></span>{/if}
								</Button>
							{/snippet}
						</DropdownMenu.Trigger>
						<DropdownMenu.Content align="end" class="w-64">
							<DropdownMenu.Label class="flex items-center gap-3 font-normal">
								<UserAvatar email={me.email} name={me.name} class="size-10" />
								<span class="grid min-w-0 gap-0.5">
									<span class="truncate font-semibold">{me.name || 'Your account'}</span>
									<span class="truncate text-muted-foreground">{me.email}</span>
								</span>
							</DropdownMenu.Label>
							<DropdownMenu.Separator />
							{#if me.role === 'admin'}
								<DropdownMenu.Item onSelect={() => goto('/admin/users')}>
									<ShieldUserIcon class="text-muted-foreground" />Admin
									{#if openReports.count}<Badge
											class="ml-auto h-4.5 min-w-4.5 bg-destructive px-1 text-background tabular-nums"
											>{openReports.count}<span class="sr-only"> open reports</span></Badge
										>{/if}
								</DropdownMenu.Item>
							{/if}
							<DropdownMenu.Item onSelect={() => goto('/dashboard/settings')}
								><SettingsIcon class="text-muted-foreground" />Settings</DropdownMenu.Item
							>
							<DropdownMenu.Item onSelect={() => goto('/report')}
								><FlagIcon class="text-muted-foreground" />Report abuse</DropdownMenu.Item
							>
							<DropdownMenu.Separator />
							<DropdownMenu.RadioGroup
								value={userPrefersMode.current}
								onValueChange={(v) => setMode(v as 'light' | 'dark' | 'system')}
							>
								<DropdownMenu.GroupHeading class="font-normal">Theme</DropdownMenu.GroupHeading>
								<DropdownMenu.RadioItem value="light"
									><SunIcon class="text-muted-foreground" />Light</DropdownMenu.RadioItem
								>
								<DropdownMenu.RadioItem value="dark"
									><MoonIcon class="text-muted-foreground" />Dark</DropdownMenu.RadioItem
								>
								<DropdownMenu.RadioItem value="system"
									><MonitorIcon class="text-muted-foreground" />System</DropdownMenu.RadioItem
								>
							</DropdownMenu.RadioGroup>
							<DropdownMenu.Separator />
							<DropdownMenu.Item variant="destructive" onSelect={signOut}
								><LogOutIcon />Sign out</DropdownMenu.Item
							>
						</DropdownMenu.Content>
					</DropdownMenu.Root>
				</div>
			{:else}
				<div class="ml-auto flex items-center gap-2">
					{#if page.url.pathname !== '/login'}<Button href="/login" variant="ghost">Sign in</Button
						>{/if}
					{#if providers.signup && page.url.pathname !== '/signup'}<Button href="/signup"
							>Create account</Button
						>{/if}
				</div>
			{/if}
		</div>
	</div>
</header>

{#if me}
	<!-- The server filters links and users, so bits-ui's filter is off: it sorts by moving DOM nodes, which desyncs Svelte's lists and leaves stale rows. -->
	<Command.Dialog
		bind:open={commandOpen}
		bind:value={selected}
		shouldFilter={false}
		title="Search or jump to"
		description={admin ? 'Find a link or user, or go to a page' : 'Find a link or go to a page'}
		class="sm:max-w-xl"
	>
		<Command.Input
			placeholder={admin
				? 'Search links and users, or type a command'
				: 'Search links, or type a command'}
			bind:value={search}
		>
			{#snippet end()}
				{#if searching}
					<Command.Loading>
						{#snippet child({ props })}
							<div {...props} aria-label="Searching" class="enter text-muted-foreground">
								<Spinner role="presentation" aria-hidden="true" />
							</div>
						{/snippet}
					</Command.Loading>
				{/if}
			{/snippet}
		</Command.Input>
		<Command.List>
			<Command.Empty class="py-10 text-muted-foreground"
				>No {admin ? 'links or users' : 'links'} match “{search.trim()}”.</Command.Empty
			>
			{#if loading || found?.links.length}
				<Command.Group
					heading="Links"
					class={stale ? 'opacity-60 transition-opacity' : 'transition-opacity'}
				>
					{#each linkRows as l (l.id)}
						{#if loading}
							<Command.Item value="link:{l.id}" disabled class="data-disabled:opacity-100"
								>{@render linkRow(null)}</Command.Item
							>
						{:else}
							<Command.LinkItem
								value="link:{l.id}"
								href="/dashboard/links/{l.slug}"
								onSelect={close}>{@render linkRow(l)}</Command.LinkItem
							>
						{/if}
					{/each}
				</Command.Group>
			{/if}
			{#if admin && (loading || found?.users.length)}
				<Command.Group
					heading="Users"
					class={stale ? 'opacity-60 transition-opacity' : 'transition-opacity'}
				>
					{#each userRows as u (u.id)}
						{#if loading}
							<Command.Item value="user:{u.id}" disabled class="data-disabled:opacity-100"
								>{@render userRow(null)}</Command.Item
							>
						{:else}
							<Command.LinkItem
								value="user:{u.id}"
								href="/admin/users/{u.id}?email={encodeURIComponent(u.email)}"
								onSelect={close}>{@render userRow(u)}</Command.LinkItem
							>
						{/if}
					{/each}
				</Command.Group>
			{/if}
			{#if shownPages.length}
				<Command.Group heading="Go to">
					{#each shownPages as c (c.href)}
						<Command.LinkItem value="go:{c.href}" href={c.href} onSelect={close}
							>{c.label}</Command.LinkItem
						>
					{/each}
				</Command.Group>
			{/if}
			{#if shownActions.length}
				<Command.Group heading="Actions">
					{#each shownActions as c (c.value)}
						<Command.Item value={c.value} onSelect={() => run(c.run)}>{c.label}</Command.Item>
					{/each}
				</Command.Group>
			{/if}
		</Command.List>
		<div
			class="hidden items-center gap-4 border-t px-5 py-2.5 text-xs text-muted-foreground sm:flex"
			aria-hidden="true"
		>
			<span class="flex items-center gap-1.5"
				><Kbd.Group><Kbd.Root>↑</Kbd.Root><Kbd.Root>↓</Kbd.Root></Kbd.Group>Move</span
			>
			<span class="flex items-center gap-1.5"><Kbd.Root>↵</Kbd.Root>Open</span>
			<span class="ml-auto flex items-center gap-1.5"><Kbd.Root>Esc</Kbd.Root>Close</span>
		</div>
	</Command.Dialog>

	<!-- Loading rows are these same rows with placeholder data, rendered invisibly under the shimmer. -->
	{#snippet linkRow(link: Link | null)}
		{@const l = link ?? placeholderLink}
		{@const loading = !link}
		<Skeleton {loading} class="shrink-0 rounded-md"
			><span class="grid size-8 place-items-center rounded-md border bg-card text-muted-foreground"
				><LinkIcon class="size-4" /></span
			></Skeleton
		>
		<span class="min-w-0 flex-1">
			<span class="block truncate font-mono font-bold"
				><Skeleton {loading} class="inline-block max-w-full align-top"
					><span class="font-normal text-muted-foreground"
						><span class="hidden sm:inline">{shortHost}</span>/</span
					>{l.slug}</Skeleton
				></span
			>
			<span class="block truncate font-mono text-[13px] text-muted-foreground"
				><Skeleton {loading} class="inline-block max-w-full overflow-hidden align-top"
					>{l.url}</Skeleton
				></span
			>
		</span>
		<span class="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
			{#if l.blocked}<Badge variant="stamp-destructive">blocked</Badge>{/if}
			<Skeleton {loading} class="inline-block tabular-nums"
				>{formatNumber(l.click_count)} {l.click_count === 1 ? 'click' : 'clicks'}</Skeleton
			>
		</span>
	{/snippet}

	{#snippet userRow(user: AdminUserRow | null)}
		{@const u = user ?? placeholderUser}
		{@const loading = !user}
		<Skeleton {loading} class="shrink-0 rounded-full"
			><UserAvatar email={u.email} name={u.name} class="size-8" /></Skeleton
		>
		<span class="min-w-0 flex-1">
			<span class="block truncate font-medium"
				><Skeleton {loading} class="inline-block max-w-full align-top">{u.name ?? u.email}</Skeleton
				></span
			>
			<span class="block truncate font-mono text-[13px] text-muted-foreground"
				><Skeleton {loading} class="inline-block max-w-full overflow-hidden align-top"
					>{u.email}</Skeleton
				></span
			>
		</span>
		<span class="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
			{#if u.role === 'admin'}<Skeleton {loading} class="rounded-[5px]"
					><Badge variant="stamp">admin</Badge></Skeleton
				>{/if}
			{#if u.disabled}<Skeleton {loading} class="rounded-[5px]"
					><Badge variant="stamp-destructive">disabled</Badge></Skeleton
				>{/if}
			<span class="hidden sm:inline"
				><Skeleton {loading} class="inline-block tabular-nums"
					>{formatNumber(u.link_count)} {u.link_count === 1 ? 'link' : 'links'}</Skeleton
				></span
			>
		</span>
	{/snippet}
{/if}
