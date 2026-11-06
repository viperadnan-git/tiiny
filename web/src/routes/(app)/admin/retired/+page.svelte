<script lang="ts">
	import { shortHost } from '#lib/origin.ts';
	import { toast } from 'svelte-sonner';
	import { api, qs } from '#lib/api-client.ts';
	import { toastError } from '#lib/toast.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import ListHeader from '#lib/components/ListHeader.svelte';
	import ListStatus from '#lib/components/ListStatus.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import * as Item from '#lib/components/ui/item/index.js';
	import { confirm } from '#lib/confirm.svelte.ts';
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { paged } from '#lib/paged.svelte.ts';
	import { placeholderRetired, skeletonRows } from '#lib/placeholders.ts';
	import type { RetiredSlug, RetiredSlugPage } from '#lib/types.ts';

	let busy = $state<number | null>(null);
	const list = paged<RetiredSlug>(async (query, cursor) => {
		const page = await api<RetiredSlugPage>('GET', `/admin/retired-slugs${qs({ query, cursor })}`);
		return { items: page.slugs, next_cursor: page.next_cursor };
	});

	async function release(r: RetiredSlug) {
		const ok = await confirm({
			title: `Release /${r.slug}?`,
			description:
				'Anyone can claim it again. Browsers that followed its old permanent redirect may keep going to the old destination.',
			action: 'Release slug',
			cancel: 'Keep retired'
		});
		if (!ok) return;
		busy = r.id;
		try {
			await api('DELETE', `/admin/retired-slugs/${encodeURIComponent(r.slug)}`);
			list.items = list.items.filter((x) => x.id !== r.id);
			toast.success(`/${r.slug} released`, { description: 'Anyone can claim it now.' });
		} catch (e) {
			toastError(`Couldn't release /${r.slug}`, e);
		} finally {
			busy = null;
		}
	}
</script>

<svelte:head><title>Retired slugs · Admin · tiiny</title></svelte:head>

<div class="grid gap-6">
	<ListHeader
		title="Retired slugs"
		description="Deleting a permanent link retires its slug, so it answers “gone” and can't be reused. Release one to make it available again."
		search={{ id: 'retired-search', label: 'Search retired slugs', placeholder: 'Search by slug' }}
		bind:value={list.search}
	/>

	<FormAlert message={list.error} />

	{#if list.items.length || list.skeleton}
		{@const rows = list.rows(skeletonRows(placeholderRetired))}
		<Card.Root class="gap-0 py-0" aria-busy={list.loading}>
			<ul>
				{#each rows as r, i (r.id)}
					{@const loading = r.id < 0}
					<li class={i ? 'border-t' : ''}>
						<Item.Root class="flex-nowrap gap-3 rounded-none px-4 py-3.5 sm:gap-4 sm:px-6">
							<Item.Content class="min-w-0 gap-0.5">
								<p class="slug truncate text-[18px]">
									<Skeleton {loading} class="inline-block max-w-full"
										><span class="host">{shortHost}/</span>{r.slug}</Skeleton
									>
								</p>
								<p class="font-mono text-[13px] text-muted-foreground">
									<Skeleton {loading} class="inline-block"
										>Retired <RelativeTime unix={r.retired_at} /></Skeleton
									>
								</p>
							</Item.Content>
							<Item.Actions class="shrink-0">
								<Skeleton {loading} class="rounded-full">
									<Button
										size="sm"
										variant="secondary"
										disabled={busy === r.id || loading}
										aria-label="Release /{r.slug}"
										onclick={() => release(r)}>Release</Button
									>
								</Skeleton>
							</Item.Actions>
						</Item.Root>
					</li>
				{/each}
			</ul>
		</Card.Root>
	{/if}

	<ListStatus {list} hint="Search matches part of a slug.">
		{#snippet empty()}
			<Empty.Root class="border-border">
				<Empty.Header>
					<Empty.Title>No retired slugs</Empty.Title>
					<Empty.Description>Slugs of deleted permanent links show here.</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{/snippet}
	</ListStatus>
</div>
