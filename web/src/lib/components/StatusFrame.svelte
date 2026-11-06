<script lang="ts">
	import type { Snippet } from 'svelte';
	import Logo from '#lib/components/Logo.svelte';

	let { children }: { children: Snippet } = $props();
</script>

<svelte:head><meta name="robots" content="noindex" /></svelte:head>

<header class="pt-2 sm:pt-3">
	<div class="wrap">
		<div class="flex h-14 items-center rounded-full border bg-card px-4 shadow-paper sm:px-6">
			<Logo class="text-[24px]" />
		</div>
	</div>
</header>
<main
	class="wrap grid min-h-[calc(100dvh-68px)] content-center gap-10 py-12 sm:gap-12 sm:py-16 md:grid-cols-[1fr_1fr] md:items-center"
>
	{@render children()}
</main>
