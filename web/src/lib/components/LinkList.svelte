<script lang="ts">
	import { page } from '$app/state';
	import { api, qs } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { paged } from '#lib/paged.svelte.ts';
	import { SKELETON_ROWS } from '#lib/placeholders.ts';
	import type { Snippet } from 'svelte';
	import type { Link, LinkPage } from '#lib/types.ts';
	import EditLinkDialog from './EditLinkDialog.svelte';
	import LinkLabel from './LinkLabel.svelte';
	import ListHeader from './ListHeader.svelte';
	import ListStatus from './ListStatus.svelte';

	let {
		endpoint,
		params = {},
		links = $bindable([]),
		fresh = null,
		title = 'Your links',
		heading = 'h2',
		toolbar,
		empty
	}: {
		endpoint: string;
		params?: Parameters<typeof qs>[0];
		links?: Link[];
		fresh?: number | null;
		title?: string;
		heading?: 'h1' | 'h2';
		/** Filters, under the heading. */
		toolbar?: Snippet;
		empty: Snippet;
	} = $props();

	let editing = $state<Link | null>(null);
	const admin = $derived(page.data.me?.role === 'admin');
	const list = paged<Link>(
		async (query, cursor) => {
			const page = await api<LinkPage>('GET', endpoint + qs({ ...params, query, cursor }));
			return { items: page.links, next_cursor: page.next_cursor };
		},
		{ get: () => links, set: (v) => (links = v) }
	);
</script>

<section aria-labelledby="links-heading" class="grid gap-5">
	<ListHeader
		{title}
		{heading}
		headingId="links-heading"
		search={{ id: 'link-search', label: 'Search links', placeholder: 'Search by slug or URL' }}
		bind:value={list.search}
	/>

	{@render toolbar?.()}

	<FormAlert message={list.error} />

	{#if links.length || list.skeleton}
		<ul class="grid gap-5" aria-busy={list.loading}>
			{#each list.rows(Array<null>(SKELETON_ROWS).fill(null)) as link, i (link?.id ?? `s${i}`)}
				{#if link}
					<LinkLabel
						{link}
						{admin}
						fresh={link.id === fresh}
						onchange={(l) => (links[i] = l)}
						onremove={(id) => (links = links.filter((x) => x.id !== id))}
						onedit={(l) => (editing = l)}
					/>
				{:else}
					<LinkLabel link={null} onchange={() => {}} onremove={() => {}} onedit={() => {}} />
				{/if}
			{/each}
		</ul>
	{/if}

	<ListStatus
		{list}
		hint="Search looks at slugs and destinations. Try a shorter part of either."
		{empty}
	/>
</section>

<EditLinkDialog
	bind:link={editing}
	onsaved={(l) => (links = links.map((x) => (x.id === l.id ? l : x)))}
/>
