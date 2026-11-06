<!-- Privacy and Terms: the heading on the page, the text on a sheet of paper. -->
<script lang="ts">
	import * as Card from '#lib/components/ui/card/index.js';
	import type { Snippet } from 'svelte';

	let { title, notice, children }: { title: string; notice?: Snippet; children: Snippet } =
		$props();
</script>

<div class="wrap pt-14 pb-20 md:pt-22 md:pb-24">
	<h1 class="max-w-[14ch] text-[clamp(40px,5.6vw,76px)] leading-[1.02] tracking-tight">{title}</h1>
	{#if notice}<div class="mt-8 max-w-[68ch]">{@render notice()}</div>{/if}
	<Card.Root
		class="prose-paper mt-8 block max-w-205 rounded-3xl px-6 py-8 text-[17px] shadow-paper *:first:mt-0 sm:px-12 sm:py-12"
	>
		{@render children()}
	</Card.Root>
</div>
