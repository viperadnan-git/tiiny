<script lang="ts">
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import EditLinkDialog from '#lib/components/EditLinkDialog.svelte';
	import LinkActions from '#lib/components/LinkActions.svelte';
	import { ApiError, api, errorMessage } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import StatusPage from '#lib/components/StatusPage.svelte';
	import { statusCopy } from '#lib/status.ts';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import * as Breadcrumb from '#lib/components/ui/breadcrumb/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import ArrowUpRightIcon from '@lucide/svelte/icons/arrow-up-right';
	import AddressLabel from '#lib/components/AddressLabel.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import CopyButton from '#lib/components/CopyButton.svelte';
	import Stamp from '#lib/components/Stamp.svelte';
	import * as Card from '#lib/components/ui/card/index.js';
	import { placeholderLink } from '#lib/placeholders.ts';
	import BarList from './BarList.svelte';
	import ClicksChart from './ClicksChart.svelte';
	import { formatNumber } from '#lib/format.ts';
	import { shortHost, shortUrl } from '#lib/origin.ts';
	import type { Link, LinkStats, StatsRange } from '#lib/types.ts';

	const ranges: { value: StatsRange; label: string }[] = [
		{ value: '7d', label: '7 days' },
		{ value: '30d', label: '30 days' },
		{ value: '90d', label: '90 days' },
		{ value: 'all', label: 'All time' }
	];

	const slug = $derived(page.params.slug ?? '');
	let range = $state<StatsRange>('30d');
	let link = $state<Link | null>(null);
	let stats = $state<LinkStats | null>(null);
	let statsError = $state<string | null>(null);
	let notFound = $state(false);
	/** The link couldn't load (not a 404): the page shows the shared status screen. */
	let failed = $state<ReturnType<typeof statusCopy> | null>(null);
	let attempt = $state(0);
	const error = $derived(notFound || failed ? null : statsError);
	const l = $derived(link ?? placeholderLink);
	const admin = $derived(page.data.me?.role === 'admin');
	let editing = $state<Link | null>(null);

	// The destination is one truncated line; "Show full" appears only when it's cut off.
	let urlEl = $state<HTMLElement | null>(null);
	let fullUrl = $state(false);
	let clipped = $state(false);
	$effect(() => {
		void slug;
		fullUrl = false;
	});
	$effect(() => {
		const el = urlEl;
		void l.url;
		if (!el || fullUrl) return;
		const check = () => (clipped = el.scrollWidth > el.clientWidth);
		check();
		const ro = new ResizeObserver(check);
		ro.observe(el);
		return () => ro.disconnect();
	});

	function saved(next: Link) {
		link = next;
		// A new slug is a new address for this page.
		if (next.slug !== slug) void goto(`/dashboard/links/${next.slug}`, { replaceState: true });
	}

	$effect(() => {
		const s = slug;
		void attempt;
		[link, notFound, failed] = [null, false, null];
		api<Link>('GET', `/links/${s}`)
			.then((l) => s === slug && (link = l))
			.catch((e) => {
				if (s !== slug) return;
				notFound = e instanceof ApiError && e.status === 404;
				if (!notFound)
					failed = statusCopy(
						e instanceof ApiError ? e.status : null,
						e instanceof ApiError ? errorMessage(e) : ''
					);
			});
	});

	$effect(() => {
		const [s, r] = [slug, range];
		void attempt;
		// A new range or link shows the skeleton, not the previous numbers.
		[stats, statsError] = [null, null];
		const current = () => s === slug && r === range;
		api<LinkStats>('GET', `/links/${s}/stats?range=${r}`)
			.then((s) => current() && (stats = s))
			.catch((e) => current() && (statsError = errorMessage(e)));
	});

	const regionNames = new Intl.DisplayNames(undefined, { type: 'region' });
	const word = (v: string) => (/^[a-z]+$/.test(v) ? v[0].toUpperCase() + v.slice(1) : v);
	const country = (v: string) => {
		if (v === 'unknown') return 'Unknown';
		try {
			return regionNames.of(v) ?? v;
		} catch {
			return v;
		}
	};
	const rangeTotal = $derived(stats ? stats.series.reduce((n, d) => n + d.count, 0) : 0);
	const rangeLabel = $derived(ranges.find((r) => r.value === range)?.label.toLowerCase());
</script>

<svelte:head
	><title>{failed ? failed.title : notFound ? 'Link not found' : `${slug} · Stats`} · tiiny</title
	></svelte:head
>

<div class="grid gap-8">
	<Breadcrumb.Root>
		<Breadcrumb.List>
			<Breadcrumb.Item><Breadcrumb.Link href="/dashboard">Links</Breadcrumb.Link></Breadcrumb.Item>
			<Breadcrumb.Separator />
			<Breadcrumb.Item
				><Breadcrumb.Page class="font-mono"
					><Skeleton loading={!link && !notFound && !failed} class="inline-block"
						>{link?.slug ?? slug}</Skeleton
					></Breadcrumb.Page
				></Breadcrumb.Item
			>
		</Breadcrumb.List>
	</Breadcrumb.Root>

	<FormAlert message={error} />

	{#if failed}
		<div class="grid gap-10 py-4 sm:gap-12 md:grid-cols-2 md:items-center">
			<StatusPage
				code={failed.code}
				mark={failed.mark}
				title={failed.title}
				text={failed.text}
				label="Returned from"
				dest={page.url.pathname}
			>
				{#snippet actions()}
					{#if failed?.retry}
						<Button onclick={() => attempt++}>Try again</Button>
						<a
							class="text-[15px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
							href="/dashboard">Back to links</a
						>
					{:else}
						<Button href="/dashboard">Back to links</Button>
					{/if}
				{/snippet}
			</StatusPage>
		</div>
	{:else if notFound}
		<!-- Undeliverable: the address struck through, the reasons a postal return label would list. -->
		<AddressLabel ticket>
			<h1 class="slug min-w-0 text-[clamp(26px,4.2vw,46px)] break-all">
				<span class="sr-only">No link at </span><del class="decoration-stamp decoration-2"
					><span class="host block pb-1 text-[0.6em] sm:inline sm:p-0 sm:text-[1em]"
						>{shortHost}/</span
					>{slug}</del
				>
			</h1>
			<p class="mt-3 text-[13px] text-muted-foreground">
				Returned to sender. None of your links has this address. It may have been:
			</p>
			<ul class="mt-2 grid max-w-136 gap-1 text-[15px] sm:grid-cols-2 sm:gap-x-6">
				{#each ['never created', 'renamed to a new slug', 'deleted', 'made by another account'] as reason (reason)}
					<li
						class="flex items-baseline gap-2.5 before:size-2 before:shrink-0 before:-translate-y-px before:rounded-xs before:border-[1.5px] before:border-stamp"
					>
						{reason}
					</li>
				{/each}
			</ul>
			<p class="mt-3 text-[13px] text-muted-foreground">
				Slugs are case-sensitive: /Abc and /abc are different links.
			</p>
			{#snippet stamp()}
				<Stamp label="Returned" value="404" rotate={-6} land />
			{/snippet}
			{#snippet footer()}
				<Button size="sm" href="/dashboard">Back to links</Button>
			{/snippet}
		</AddressLabel>
	{:else}
		<AddressLabel ticket>
			<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
				<h1 class="slug min-w-0 text-[clamp(26px,4.2vw,46px)]">
					<span class="host block pb-1 text-[0.6em] sm:inline sm:p-0 sm:text-[1em]"
						><Skeleton loading={!link} class="inline-block">{shortHost}/</Skeleton></span
					><Skeleton loading={!link} class="inline-block">{l.slug}</Skeleton>
				</h1>
				{#if l.permanent}<Badge variant="stamp">permanent</Badge>{/if}
				{#if l.blocked}<Badge variant="stamp-destructive" title="Blocked by an admin">blocked</Badge
					>{/if}
			</div>
			<div class="mt-3 flex items-baseline justify-between gap-3 text-[13px]">
				<p class="whitespace-nowrap text-muted-foreground">Redirects to</p>
				{#if link && (clipped || fullUrl)}
					<Button
						variant="link"
						class="h-auto shrink-0 border-0 p-0 text-[13px] leading-[inherit] whitespace-nowrap"
						aria-expanded={fullUrl}
						aria-controls="destination"
						onclick={() => (fullUrl = !fullUrl)}
						>{#if fullUrl}Show less{:else}Show full{/if}</Button
					>
				{/if}
			</div>
			<p
				id="destination"
				bind:this={urlEl}
				class="mt-0.5 font-mono text-[14px] sm:text-[15px] {fullUrl
					? 'max-h-52 overflow-y-auto overscroll-contain break-all'
					: 'truncate'}"
			>
				<Skeleton loading={!link} class="inline-block max-w-full overflow-hidden"
					><a
						class="text-muted-foreground hover:text-foreground"
						href={l.url}
						rel="noopener noreferrer"
						title={l.url}>{l.url}</a
					></Skeleton
				>
			</p>
			{#snippet stamp()}
				<Skeleton loading={!link} class="-rotate-6 rounded-lg">
					<Stamp
						label="Clicks"
						value={stats?.click_count ?? l.click_count}
						rotate={link ? -6 : 0}
						land={!!link}
					/>
				</Skeleton>
				<span class="font-mono text-[11px] leading-tight text-muted-foreground sm:text-[13px]"
					><Skeleton loading={!link} class="inline-block"
						><RelativeTime unix={l.created_at} /></Skeleton
					></span
				>
			{/snippet}
			{#snippet footer()}
				<div class="flex flex-wrap items-center gap-1 sm:gap-2">
					<Skeleton loading={!link} class="rounded-full"
						><CopyButton
							value={shortUrl(l.slug)}
							aria-label="Copy {shortUrl(l.slug)}"
							compact
						/></Skeleton
					>
					<Skeleton loading={!link} class="rounded-full"
						><Button
							size="sm"
							variant="ghost"
							href={shortUrl(l.slug)}
							target="_blank"
							rel="noopener"
							class="max-sm:px-2.5">Open<ArrowUpRightIcon /></Button
						></Skeleton
					>
					<Skeleton loading={!link} class="rounded-full">
						<LinkActions
							link={l}
							{admin}
							disabled={!link}
							onchange={(next) => (link = next)}
							onremove={() =>
								goto(admin && l.user_id !== page.data.me?.id ? '/admin/links' : '/dashboard')}
							onedit={(next) => (editing = next)}
						/>
					</Skeleton>
				</div>
			{/snippet}
		</AddressLabel>

		<section class="grid gap-5" aria-label="Clicks" aria-busy={!stats && !statsError}>
			<div class="flex flex-wrap items-end justify-between gap-4">
				<div>
					<p class="text-sm text-muted-foreground">
						{range === 'all' ? 'Since the first click' : `Last ${rangeLabel}`}
					</p>
					<p class="font-serif text-[44px] leading-tight font-medium">
						<Skeleton loading={!stats && !statsError} class="inline-block"
							>{stats ? formatNumber(rangeTotal) : statsError ? '–' : '000'}</Skeleton
						> <span class="text-[20px] text-muted-foreground">clicks</span>
					</p>
				</div>
				<Tabs.Root bind:value={range} class="max-w-full overflow-x-auto">
					<Tabs.List aria-label="Date range">
						{#each ranges as r (r.value)}
							<Tabs.Trigger value={r.value}>{r.label}</Tabs.Trigger>
						{/each}
					</Tabs.List>
				</Tabs.Root>
			</div>
			{#if l.permanent}
				<p class="text-sm text-muted-foreground">
					Permanent links redirect with 301, so repeat visits from the same browser aren't counted.
				</p>
			{/if}
			{#if stats || !statsError}
				<Card.Root class="rounded-3xl p-5 sm:p-6"
					><ClicksChart series={stats?.series ?? null} /></Card.Root
				>
				<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
					<BarList
						title="Referrers"
						items={stats?.referrers ?? null}
						label={(v) => (v === 'direct' || v === 'other' ? word(v) : v)}
					/>
					<BarList title="Countries" items={stats?.countries ?? null} label={country} />
					<BarList title="Devices" items={stats?.devices ?? null} label={word} />
					<BarList title="Browsers" items={stats?.browsers ?? null} label={word} />
					<BarList title="Operating systems" items={stats?.oses ?? null} label={word} />
				</div>
				<p class="text-sm text-muted-foreground">
					Clicks are counted in batches and can take a few seconds to appear.
				</p>
			{/if}
		</section>
	{/if}
</div>

<EditLinkDialog bind:link={editing} onsaved={saved} />
