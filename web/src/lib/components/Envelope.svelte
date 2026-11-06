<!--
	Ruled lines ported from Opensource UI "Journal Writing" (components/text/journal-writing-card.tsx),
	https://opensourceui.in/components/journal-writing. MIT License, Copyright (c) 2026 Bidyut Kundu.
	Changes: Svelte 5, lines in the Paper rule token, used on the sign-in envelope.
-->
<!-- The auth form's envelope: the addresses fill in as you type, and the stamp lands once the form is valid. -->
<script lang="ts">
	import * as Card from '#lib/components/ui/card/index.js';
	import Stamp from '#lib/components/Stamp.svelte';
	import { shortHost } from '#lib/origin.ts';

	/** `strong` bolds the line once it has text; empty lines show `placeholder`, muted. */
	type Line = { text: string; placeholder?: string; strong?: boolean };

	let {
		from,
		to = [{ text: 'Your links', strong: true }, { text: `${shortHost}/dashboard` }],
		valid,
		mark,
		facts,
		full = false
	}: {
		from: Line[];
		to?: Line[];
		valid: boolean;
		mark: { label: string; value: string };
		facts: string[];
		full?: boolean;
	} = $props();
	const lines =
		'bg-[repeating-linear-gradient(transparent,transparent_27px,var(--rule)_27px,var(--rule)_28px)] font-mono text-[15px] leading-7';
</script>

{#snippet address(ls: Line[])}
	{#each ls as l, i (i)}
		<p class="truncate {l.strong && l.text.trim() ? 'font-bold' : 'text-muted-foreground'}">
			{l.text.trim() || l.placeholder || '\u00a0'}
		</p>
	{/each}
{/snippet}

{#snippet slot()}
	{#if valid}
		<Stamp label={mark.label} value={mark.value} rotate={-9} land />
	{:else}
		<span
			class="block h-15.5 w-21 rotate-[-9deg] rounded-lg border-[2.5px] border-dashed border-border sm:h-18 sm:w-26"
		></span>
	{/if}
{/snippet}

{#if !full}
	<!-- Phones: a slim return-address strip above the form. -->
	<Card.Root class="flex-row items-center gap-4 px-5 py-4">
		<div class="min-w-0 flex-1">
			<p class="text-[13px] text-muted-foreground">Return address</p>
			<div class="mt-1 {lines}">
				{@render address(from)}
			</div>
		</div>
		<div class="shrink-0">{@render slot()}</div>
	</Card.Root>
{:else}
	<!-- Wide screens: the whole envelope, with an airmail border. -->
	<div class="flex flex-col gap-6">
		<div
			class="rounded-[28px] bg-[repeating-linear-gradient(135deg,var(--stamp)_0_14px,transparent_14px_28px,var(--destructive)_28px_42px,transparent_42px_56px)] p-2.5 shadow-paper"
		>
			<div class="grid aspect-[1.55] grid-rows-[auto_1fr] rounded-[20px] bg-card p-8 xl:p-10">
				<div class="flex items-start justify-between gap-6">
					<div class="w-[min(240px,55%)]">
						<p class="text-[13px] text-muted-foreground">Return address</p>
						<div class="mt-1 {lines}">
							{@render address(from)}
						</div>
					</div>
					{@render slot()}
				</div>
				<div class="self-center justify-self-center pl-[18%]">
					<p class="text-[13px] text-muted-foreground">Deliver to</p>
					<div class="mt-1 w-65 {lines} text-[17px]">
						{@render address(to)}
						<p aria-hidden="true">&nbsp;</p>
					</div>
				</div>
			</div>
		</div>
		<ul class="grid gap-3 px-2">
			{#each facts as f (f)}
				<li class="flex gap-3 text-[15px] text-muted-foreground">
					<span class="mt-2.25 size-1.5 shrink-0 rounded-full bg-stamp" aria-hidden="true"
					></span>{f}
				</li>
			{/each}
		</ul>
	</div>
{/if}
