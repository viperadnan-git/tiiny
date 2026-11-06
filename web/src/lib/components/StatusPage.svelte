<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import AddressLabel from '#lib/components/AddressLabel.svelte';
	import Stamp from '#lib/components/Stamp.svelte';

	let {
		code,
		mark,
		title,
		text,
		dest,
		label = 'Redirects to',
		actions
	}: {
		code: number | string;
		mark: string;
		title: string;
		text: string;
		dest: string;
		label?: string;
		actions?: Snippet;
	} = $props();
</script>

<svelte:head><title>{title} · tiiny</title></svelte:head>

<div>
	<h1 class="max-w-[14ch] text-[clamp(40px,5vw,64px)] leading-[1.05]">{title}</h1>
	<p class="mt-5 max-w-[44ch] text-[17px] text-muted-foreground">{text}</p>
	<div class="mt-8 flex flex-wrap items-center gap-x-6 gap-y-3">
		{#if actions}
			{@render actions()}
		{:else}
			<Button href="/">Go to tiiny</Button>
			<a
				class="text-[15px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
				href="/report">Report abuse</a
			>
		{/if}
	</div>
</div>
<div aria-hidden="true">
	<AddressLabel class="rotate-[-1.5deg]">
		<p class="slug text-[clamp(30px,9vw,40px)]"><span class="host">/</span>{code}</p>
		<p class="mt-3 text-[13px] text-muted-foreground">{label}</p>
		<p class="mt-0.5 truncate font-mono text-[15px] text-muted-foreground">{dest}</p>
		{#snippet stamp()}
			<Stamp label={mark} value={String(code)} rotate={-8} />
		{/snippet}
	</AddressLabel>
</div>
