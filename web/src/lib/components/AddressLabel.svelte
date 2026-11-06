<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as Card from '#lib/components/ui/card/index.js';
	import { cn } from '#lib/utils.ts';

	let {
		children,
		stamp,
		footer,
		ticket = false,
		class: className
	}: {
		children: Snippet;
		stamp: Snippet;
		footer?: Snippet;
		ticket?: boolean;
		class?: string;
	} = $props();
</script>

{#if ticket}
	<!-- Ticket-shaped label: the short link, then the rubber stamp on a perforated side stub, narrower on phones. The mask clips box-shadow, so depth comes from a drop-shadow on this wrapper. -->
	<div class={cn('drop-shadow-[0_8px_10px_rgba(26,36,33,.07)] dark:drop-shadow-none', className)}>
		<Card.Root
			class="ticket relative grid grid-cols-[minmax(0,1fr)_var(--stub)] gap-0 rounded-lg py-0 [--stub:100px] sm:[--stub:148px]"
		>
			<div class="min-w-0 border-r-2 border-dashed border-border py-4 pr-3 pl-4 sm:px-7 sm:py-6">
				{@render children()}
				{#if footer}<div class="mt-3 sm:mt-4">{@render footer()}</div>{/if}
			</div>
			<div
				class="grid content-center justify-items-center gap-2 px-1.5 py-3 text-center sm:gap-3 sm:p-4"
			>
				{@render stamp()}
			</div>
		</Card.Root>
	</div>
{:else}
	<!-- An address label: the short link on the left, its rubber stamp (click count) in the corner. -->
	<Card.Root
		class={cn(
			'grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-3 gap-y-0 rounded-lg px-5 py-5 shadow-paper sm:gap-x-5 sm:px-7 sm:py-6',
			className
		)}
	>
		<div class="min-w-0">{@render children()}</div>
		<div class="pt-1">{@render stamp()}</div>
	</Card.Root>
{/if}
