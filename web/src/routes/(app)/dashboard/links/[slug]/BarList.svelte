<script lang="ts">
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { formatNumber } from '#lib/format.ts';
	import type { ValueCount } from '#lib/types.ts';

	/** `items` is `null` while loading. */
	let {
		title,
		items,
		label = (v) => v
	}: { title: string; items: ValueCount[] | null; label?: (v: string) => string } = $props();

	// Every state reserves three rows (invisible fillers in the same markup), so loading, empty and short lists are one height.
	const minRows = 3;
	const loading = $derived(items === null);
	const filler = (n: number) =>
		Array.from({ length: n }, (_, i) => ({ value: `filler-${i}`, count: 100, filler: true }));
	const rows = $derived(
		items === null
			? filler(minRows).map((r) => ({ ...r, filler: false }))
			: [...items.map((i) => ({ ...i, filler: false })), ...filler(minRows - items.length)]
	);
	const max = $derived(Math.max(1, ...(items ?? []).map((i) => i.count)));
</script>

<Card.Root class="gap-4 rounded-3xl sm:[--card-spacing:--spacing(6)]">
	<Card.Header><Card.Title><h3 class="text-xl">{title}</h3></Card.Title></Card.Header>
	<Card.Content class="relative block">
		<ul class="grid gap-3">
			{#each rows as item (item.value)}
				<li
					class="grid gap-1.5 {item.filler ? 'invisible' : ''}"
					aria-hidden={item.filler || loading || undefined}
				>
					<div class="flex items-baseline justify-between gap-3 text-[15px]">
						<span class="min-w-0 truncate" title={item.filler || loading ? undefined : item.value}>
							<Skeleton {loading} class="inline-block"
								>{item.filler || loading ? 'example.com' : label(item.value)}</Skeleton
							>
						</span>
						<span class="font-mono text-muted-foreground tabular-nums"
							><Skeleton {loading} class="inline-block">{formatNumber(item.count)}</Skeleton></span
						>
					</div>
					<span class="h-1.5 rounded-full bg-muted" aria-hidden="true">
						{#if !loading}<span
								class="block h-full rounded-full bg-stamp"
								style:width="{(item.count / max) * 100}%"
							></span>{/if}
					</span>
				</li>
			{/each}
		</ul>
		{#if items?.length === 0}
			<p class="absolute inset-x-0 top-0 text-[15px] text-muted-foreground">
				No clicks in this range yet.
			</p>
		{/if}
	</Card.Content>
</Card.Root>
