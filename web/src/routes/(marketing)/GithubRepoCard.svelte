<!--
	Ported from Opensource UI "GitHub Repo" (components/socials/github-repo-card.tsx),
	https://opensourceui.in/components/github-repo. MIT License, Copyright (c) 2026 Bidyut Kundu.
	Changes: Svelte 5, Paper tokens, no live or fake counts (no star toggle, no forks); static facts and real links instead.

	Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
	documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
	rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
	persons to whom the Software is furnished to do so, subject to the following conditions:

	The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
	Software.

	THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
	WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
	COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
	OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
-->
<script lang="ts">
	import BrandIcon from '#lib/components/BrandIcon.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { cn } from '#lib/utils.ts';

	let {
		url,
		description,
		facts,
		class: className
	}: {
		url: string;
		description: string;
		facts: { label: string; value: string }[];
		class?: string;
	} = $props();

	// "https://github.com/owner/name" → owner, name; falls back to the whole path.
	const parts = $derived(new URL(url).pathname.split('/').filter(Boolean));
	const owner = $derived(parts[0] ?? '');
	const name = $derived(parts[1] ?? '');
</script>

<Card.Root data-slot="github-repo-card" class={cn('gap-0 p-6 shadow-paper sm:p-7', className)}>
	<div class="flex items-start justify-between gap-3">
		<a href={url} class="flex min-w-0 items-center gap-2 hover:text-stamp">
			<BrandIcon name="github" class="size-5" />
			<span class="truncate font-mono text-[15px]"
				>{owner} <span class="text-muted-foreground">/</span> <b>{name}</b></span
			>
		</a>
		<Badge variant="outline" class="text-muted-foreground">Public</Badge>
	</div>
	<p class="mt-3 max-w-[48ch] text-muted-foreground">{description}</p>
	<dl class="mt-5 grid grid-cols-2 gap-x-6 gap-y-3 border-t pt-4 sm:grid-cols-4">
		{#each facts as f (f.label)}
			<div>
				<dt class="text-[13px] text-muted-foreground">{f.label}</dt>
				<dd class="mt-0.5 font-mono text-[15px] font-bold">{f.value}</dd>
			</div>
		{/each}
	</dl>
	<div class="mt-6 flex flex-wrap gap-2">
		<Button href={url} size="sm">View on GitHub</Button>
		<Button href="{url.replace(/\/$/, '')}/issues" size="sm" variant="secondary"
			>Open an issue</Button
		>
	</div>
</Card.Root>
