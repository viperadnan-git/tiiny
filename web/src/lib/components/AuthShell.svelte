<script lang="ts">
	import type { Snippet } from 'svelte';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';

	let {
		title,
		intro,
		children,
		footer,
		label,
		loading = false
	}: {
		title: string;
		intro?: string;
		children: Snippet;
		footer?: Snippet;
		label?: Snippet<[boolean]>;
		/** Title renders as a skeleton of `title`. */
		loading?: boolean;
	} = $props();
</script>

{#if label}
	<!-- Envelope on the left, form on the right; on phones the envelope shrinks to a strip above the form. -->
	<div
		class="grid gap-8 pt-2 sm:pt-6 lg:grid-cols-[minmax(0,1.05fr)_minmax(0,1fr)] lg:gap-16 xl:gap-24"
	>
		<div class="hidden lg:block lg:pt-4" aria-hidden="true">{@render label(true)}</div>
		<div class="mx-auto grid w-full max-w-115 content-start gap-6 lg:mr-0 lg:py-4">
			<div>
				<h1 class="text-[clamp(36px,5vw,56px)] leading-[1.04]">
					<Skeleton {loading} class="inline-block">{title}</Skeleton>
				</h1>
				{#if intro}<p class="mt-3 max-w-[44ch] text-muted-foreground">{intro}</p>{/if}
			</div>
			<div class="lg:hidden" aria-hidden="true">{@render label(false)}</div>
			<div class="grid gap-5">{@render children()}</div>
			{#if footer}<p class="text-[15px] text-muted-foreground">{@render footer()}</p>{/if}
		</div>
	</div>
{:else}
	<div class="mx-auto grid max-w-110 gap-6 pt-4 sm:pt-8">
		<div>
			<h1 class="text-[clamp(36px,5vw,52px)]">
				<Skeleton {loading} class="inline-block">{title}</Skeleton>
			</h1>
			{#if intro}<p class="mt-3 max-w-[52ch] text-muted-foreground">{intro}</p>{/if}
		</div>
		<Card.Root class="gap-5 px-6 py-7 sm:px-7">
			{@render children()}
		</Card.Root>
		{#if footer}<p class="text-[15px] text-muted-foreground">{@render footer()}</p>{/if}
	</div>
{/if}
