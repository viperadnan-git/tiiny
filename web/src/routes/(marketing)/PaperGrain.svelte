<!--
	Ported from Opensource UI "Fine Grain" (components/background-pattern/fine-grain-pattern.tsx),
	https://opensourceui.in/components/fine-grain-pattern. MIT License, Copyright (c) 2026 Bidyut Kundu.
	Changes: Svelte 5, dots drawn in the Paper rule/soft tokens so both themes get a quiet fibre texture.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '#lib/utils.ts';

	let { children, class: className }: { children?: Snippet; class?: string } = $props();
</script>

<div data-slot="paper-grain" class={cn('relative isolate', className)}>
	<div
		aria-hidden="true"
		class="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(circle,var(--rule)_1px,transparent_1px)] bg-size-[3px_3px] opacity-50"
	></div>
	<div
		aria-hidden="true"
		class="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(circle,var(--soft)_1px,transparent_1px)] bg-size-[5px_5px] opacity-[.07]"
	></div>
	{@render children?.()}
</div>
