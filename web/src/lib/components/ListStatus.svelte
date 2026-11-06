<!-- Shared tail of a `paged()` list: "no matches" for a search, the caller's empty state, and Load more. -->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import type { paged } from '#lib/paged.svelte.ts';

	let {
		list,
		hint,
		empty
	}: { list: ReturnType<typeof paged<unknown>>; hint: string; empty: Snippet } = $props();
	const q = $derived(list.search.trim());
</script>

{#if list.loaded && !list.loading && !list.error && !list.items.length}
	{#if q}
		<Empty.Root class="border-border">
			<Empty.Header>
				<Empty.Title>No matches for “{q}”</Empty.Title>
				<Empty.Description>{hint}</Empty.Description>
			</Empty.Header>
			<Empty.Content
				><Button variant="secondary" onclick={() => (list.search = '')}>Clear search</Button
				></Empty.Content
			>
		</Empty.Root>
	{:else}
		{@render empty()}
	{/if}
{/if}

<!-- Loading states are skeleton rows in the list itself; this only announces them. -->
{#if list.loading}<p class="sr-only" role="status">Loading</p>{/if}
{#if list.cursor !== null && !list.loading}
	<div class="flex justify-center">
		<Button variant="secondary" onclick={() => list.load(true)}>Load more</Button>
	</div>
{/if}
