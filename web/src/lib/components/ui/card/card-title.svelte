<script lang="ts">
	import { cn, type WithElementRef } from '#lib/utils.js';
	import type { HTMLAttributes } from 'svelte/elements';

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
	bind:this={ref}
	data-slot="card-title"
	class={cn(
		'font-serif text-[1.625rem] leading-tight font-medium tracking-[-0.01em] group-data-[size=sm]/card:text-xl',
		className
	)}
	{...restProps}
>
	{@render children?.()}
</div>
