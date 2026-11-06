<script lang="ts" module>
	/** Last known guest link count; the landing page clears it on sign-in. */
	export const GUEST_HINT_KEY = 'tiiny:guest-links';
</script>

<script lang="ts">
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { tick } from 'svelte';
	import { api, qs } from '#lib/api-client.ts';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import CopyButton from '#lib/components/CopyButton.svelte';
	import ListStatus from '#lib/components/ListStatus.svelte';
	import { shortHost, shortUrl } from '#lib/origin.ts';
	import { paged } from '#lib/paged.svelte.ts';
	import { placeholderLink } from '#lib/placeholders.ts';
	import type { Link, LinkPage } from '#lib/types.ts';

	let { class: className }: { class?: string } = $props();

	const limit = 5;
	// The guest cookie is HttpOnly, so the last known count sizes the skeleton before the list arrives.
	const stored = (() => {
		try {
			return localStorage.getItem(GUEST_HINT_KEY) ?? '';
		} catch {
			return '';
		}
	})();
	const hint = Math.min(limit, parseInt(stored) || 0);
	const hintMore = stored.endsWith('+');

	const list = paged<Link>(async (_q, cursor) => {
		const page = await api<LinkPage>('GET', `/links/guest${qs({ cursor, limit })}`);
		return { items: page.links, next_cursor: page.next_cursor };
	});

	$effect(() => {
		if (!list.loaded) return;
		try {
			localStorage.setItem(
				GUEST_HINT_KEY,
				`${list.items.length}${list.cursor !== null ? '+' : ''}`
			);
		} catch {
			// Only a sizing hint.
		}
	});

	let fresh = $state<{ id: number; copied: boolean } | null>(null);
	let freshButton = $state<HTMLButtonElement | null>(null);

	export async function add(link: Link, copied: boolean) {
		list.items = [link, ...list.items.filter((l) => l.id !== link.id)];
		fresh = { id: link.id, copied };
		await tick();
		freshButton?.focus();
	}

	const rows = $derived<(Link | null)[]>(list.loaded ? list.items : Array(hint).fill(null));
</script>

{#if rows.length}
	<section aria-labelledby="guest-links" class={className}>
		<h2 id="guest-links" class="text-[24px] leading-tight">Your recent links</h2>
		<!-- A routing slip: one perforated strip per link. -->
		<Card.Root class="mt-4 gap-0 py-0 shadow-paper">
			<ul>
				{#each rows as shown, i (shown?.id ?? `s${i}`)}
					{@const loading = shown === null}
					{@const link = shown ?? placeholderLink}
					<li
						class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-4 py-3.5 sm:gap-4 sm:px-6 {i
							? 'border-t-2 border-dashed border-border'
							: ''} {fresh?.id === link.id ? 'enter' : ''}"
					>
						<div class="min-w-0">
							<a
								href={loading ? undefined : shortUrl(link.slug)}
								target="_blank"
								rel="noopener"
								class="slug block truncate text-[19px] hover:text-stamp"
							>
								<span class="host block truncate text-[0.7em] sm:inline sm:text-[1em]"
									><Skeleton {loading} class="inline-block max-w-full">{shortHost}/</Skeleton></span
								><Skeleton {loading} class="inline-block">{link.slug}</Skeleton>
							</a>
							<p
								class="mt-0.5 truncate font-mono text-[13px] text-muted-foreground"
								title={loading ? undefined : link.url}
							>
								<Skeleton {loading} class="inline-block max-w-full overflow-hidden"
									>{link.url}</Skeleton
								>
							</p>
						</div>
						<Skeleton {loading} class="rounded-full">
							{#if fresh?.id === link.id}
								<CopyButton
									bind:ref={freshButton}
									value={shortUrl(link.slug)}
									copied={fresh.copied}
									aria-label="Copy {shortUrl(link.slug)}"
								/>
							{:else}
								<CopyButton value={shortUrl(link.slug)} aria-label="Copy {shortUrl(link.slug)}" />
							{/if}
						</Skeleton>
					</li>
				{/each}
			</ul>
		</Card.Root>
		<FormAlert message={list.error} class="mt-3" />
		<div class="[&:has(button,svg)]:mt-4">
			{#if !list.loaded && hintMore}
				<div class="flex justify-center">
					<Skeleton class="rounded-full"><Button variant="secondary">Load more</Button></Skeleton>
				</div>
			{:else}
				<ListStatus list={list as never} hint="">{#snippet empty()}{/snippet}</ListStatus>
			{/if}
		</div>
		<Button href="/signup" variant="secondary" class="mt-4"
			>Sign up to keep, edit and track these</Button
		>
	</section>
{/if}
