<!-- One settings group: what it is on the left, its controls on the right (stacked on phones). -->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '#lib/utils.ts';

	let {
		id,
		title,
		description,
		children,
		class: className
	}: {
		id: string;
		title: string;
		description?: string;
		children: Snippet;
		class?: string;
	} = $props();
</script>

<section
	aria-labelledby={id}
	class={cn(
		'grid scroll-mt-24 gap-5 border-t py-8 md:grid-cols-[240px_minmax(0,1fr)] md:gap-12 md:py-10',
		className
	)}
>
	<div>
		<h2 {id} class="text-[24px]">{title}</h2>
		{#if description}<p class="mt-1.5 max-w-[34ch] text-[15px] text-muted-foreground">
				{description}
			</p>{/if}
	</div>
	<div class="min-w-0">{@render children()}</div>
</section>
