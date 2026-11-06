<script lang="ts">
	import { cn, type WithElementRef } from '#lib/utils.js';
	import type { HTMLAttributes } from 'svelte/elements';

	let {
		ref = $bindable(null),
		class: className,
		loading = true,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLSpanElement>> & { loading?: boolean } = $props();
</script>

<!-- With children, the real content is rendered invisibly inside, so the block takes its exact size; once `loading` is false the children render as-is. -->
{#if !loading}
	{@render children?.()}
{:else}
	<span
		bind:this={ref}
		data-slot="skeleton"
		aria-hidden="true"
		class={cn(
			// Paper shimmer: a soft band sweeps across; hidden under reduced motion.
			'relative flex overflow-hidden rounded-md bg-muted align-top before:absolute before:inset-0 before:-translate-x-full before:animate-[shimmer_1.6s_ease-in-out_infinite] before:bg-linear-to-r before:from-transparent before:via-(--shimmer) before:to-transparent motion-reduce:before:hidden',
			className
		)}
		{...restProps}
	>
		{#if children}<span class="invisible contents" inert>{@render children()}</span>{/if}
	</span>
{/if}
