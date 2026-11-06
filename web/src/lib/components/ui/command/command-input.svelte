<script lang="ts">
	import { Command as CommandPrimitive } from 'bits-ui';
	import SearchIcon from '@lucide/svelte/icons/search';
	import type { Snippet } from 'svelte';
	import { cn } from '#lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		value = $bindable(''),
		end,
		...restProps
	}: CommandPrimitive.InputProps & { end?: Snippet } = $props();
</script>

<!-- Icon sits on the items' text edge (list p-2 + item px-3 = px-5). -->
<div data-slot="command-input-wrapper" class="flex items-center gap-3 border-b px-5">
	<SearchIcon class="size-4.5 shrink-0 text-muted-foreground" aria-hidden="true" />
	<CommandPrimitive.Input
		bind:value
		bind:ref
		data-slot="command-input"
		class={cn(
			'h-14 min-w-0 flex-1 bg-transparent text-base outline-hidden placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50 [&::-webkit-search-cancel-button]:hidden',
			className
		)}
		{...restProps}
	/>
	{@render end?.()}
</div>
