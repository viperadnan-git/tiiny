<script lang="ts">
	import ListSearch from './ListSearch.svelte';

	let {
		title,
		heading = 'h1',
		headingId,
		description,
		search,
		value = $bindable('')
	}: {
		title: string;
		heading?: 'h1' | 'h2';
		headingId?: string;
		description?: string;
		search: { id: string; label: string; placeholder: string };
		value?: string;
	} = $props();
</script>

<!-- A list's title over a rule, with its search box at the right. -->
<div class="flex flex-wrap items-end justify-between gap-4 border-b pb-4">
	<div>
		<svelte:element this={heading} id={headingId} class="text-[34px]">{title}</svelte:element>
		{#if description}<p class="mt-1 max-w-[60ch] text-sm text-muted-foreground">
				{description}
			</p>{/if}
	</div>
	<ListSearch {...search} bind:value />
</div>
