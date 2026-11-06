<script lang="ts">
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';
	import AddressLabel from '#lib/components/AddressLabel.svelte';
	import CopyButton from '#lib/components/CopyButton.svelte';
	import LinkActions from '#lib/components/LinkActions.svelte';
	import Stamp from '#lib/components/Stamp.svelte';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { shortHost, shortUrl } from '#lib/origin.ts';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { placeholderLink } from '#lib/placeholders.ts';
	import type { Link } from '#lib/types.ts';

	let {
		link: shown,
		fresh = false,
		admin = false,
		onchange,
		onremove,
		onedit
	}: {
		/** `null` renders the loading state with the same layout. */
		link: Link | null;
		fresh?: boolean;
		/** Admin view: guest ownership, Block/Unblock, View reports and owner; Edit stays available on blocked links. */
		admin?: boolean;
		onchange: (l: Link) => void;
		onremove: (id: number) => void;
		onedit: (l: Link) => void;
	} = $props();

	const loading = $derived(shown === null);
	const link = $derived(shown ?? placeholderLink);
	const host = $derived(shortHost);
</script>

<li>
	<AddressLabel ticket>
		<div class="flex flex-wrap items-center gap-x-3 gap-y-1">
			<a
				href={loading ? undefined : `/dashboard/links/${link.slug}`}
				class="slug min-w-0 text-[clamp(22px,2.6vw,28px)] hover:text-stamp"
			>
				<!-- Phones drop the host (the same on every link) so the slug gets the line. -->
				<span class="host"
					><Skeleton {loading} class="inline-block"
						><span class="hidden sm:inline">{host}</span>/</Skeleton
					></span
				><Skeleton {loading} class="inline-block">{link.slug}</Skeleton>
			</a>
			{#if link.permanent}
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}<Badge {...props} variant="stamp" tabindex={0}
								>permanent</Badge
							>{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>Redirects with 301; can't be edited</Tooltip.Content>
				</Tooltip.Root>
			{/if}
			{#if admin && link.user_id === null}<Badge variant="stamp" class="sm:hidden">guest</Badge
				>{/if}
			{#if link.blocked}
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}<Badge {...props} variant="stamp-destructive" tabindex={0}
								>blocked</Badge
							>{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content
						>{admin
							? 'Visitors see “disabled for abuse”'
							: 'Blocked by an admin. You can still delete it.'}</Tooltip.Content
					>
				</Tooltip.Root>
			{/if}
		</div>
		<!-- Guest ownership shares the label's line so admin rows keep the skeleton's height. -->
		<p class="mt-1 text-[13px] text-muted-foreground max-sm:sr-only sm:mt-2.5">
			{admin && link.user_id === null ? 'Guest link, redirects to' : 'Redirects to'}
		</p>
		<p
			class="mt-0.5 truncate font-mono text-[13px] text-muted-foreground sm:text-[14px]"
			title={loading ? undefined : link.url}
		>
			<Skeleton {loading} class="inline-block max-w-full overflow-hidden">{link.url}</Skeleton>
		</p>
		{#snippet stamp()}
			<Skeleton {loading} class="-rotate-6 rounded-lg">
				<Stamp label="Clicks" value={link.click_count} rotate={loading ? 0 : -6} land={fresh} />
			</Skeleton>
			<span class="font-mono text-[11px] leading-tight text-muted-foreground sm:text-[13px]"
				><Skeleton {loading} class="inline-block"><RelativeTime unix={link.created_at} /></Skeleton
				></span
			>
		{/snippet}
		{#snippet footer()}
			<div class="flex flex-wrap items-center gap-1 sm:gap-2">
				<Skeleton {loading} class="rounded-full"
					><CopyButton
						value={shortUrl(link.slug)}
						aria-label="Copy {shortUrl(link.slug)}"
						compact
					/></Skeleton
				>
				<Skeleton {loading} class="rounded-full"
					><Button
						size="sm"
						variant="ghost"
						href="/dashboard/links/{link.slug}"
						class="max-sm:px-2.5">Stats</Button
					></Skeleton
				>
				<Skeleton {loading} class="rounded-full">
					<LinkActions {link} {admin} disabled={loading} {onchange} {onremove} {onedit} />
				</Skeleton>
			</div>
		{/snippet}
	</AddressLabel>
</li>
