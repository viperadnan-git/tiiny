<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import * as ToggleGroup from '#lib/components/ui/toggle-group/index.js';
	import LinkList from '#lib/components/LinkList.svelte';

	const owners = [
		{ value: 'all', label: 'All', empty: 'Short links people make show here.' },
		{ value: 'user', label: 'Accounts', empty: 'Links made by signed-in users show here.' },
		{
			value: 'guest',
			label: 'Guests',
			empty: 'Links made without an account show here until someone signs up and claims them.'
		}
	] as const;
	type Owner = (typeof owners)[number]['value'];

	// In the URL, so a filtered view can be shared and survives a reload.
	const owner = $derived<Owner>(
		owners.find((o) => o.value === page.url.searchParams.get('owner'))?.value ?? 'all'
	);
	const params = $derived(owner === 'all' ? {} : { guest: owner === 'guest' ? 'true' : 'false' });

	function setOwner(v: string) {
		if (!v || v === owner) return;
		const url = new URL(page.url.href);
		if (v === 'all') url.searchParams.delete('owner');
		else url.searchParams.set('owner', v);
		void goto(url, { replace: true, reset: false });
	}
</script>

<svelte:head><title>Links · Admin · tiiny</title></svelte:head>

{#key owner}
	<LinkList endpoint="/admin/links" {params} title="Links" heading="h1">
		{#snippet toolbar()}
			<ToggleGroup.Root
				type="single"
				value={owner}
				onValueChange={setOwner}
				aria-label="Show links by owner"
				class="w-full bg-card sm:w-auto sm:justify-self-start"
			>
				{#each owners as o (o.value)}
					<ToggleGroup.Item value={o.value} class="h-10 flex-1 sm:flex-none"
						>{o.label}</ToggleGroup.Item
					>
				{/each}
			</ToggleGroup.Root>
		{/snippet}
		{#snippet empty()}
			<Empty.Root class="border-border">
				<Empty.Header>
					<Empty.Title
						>No {owner === 'all' ? '' : owner === 'guest' ? 'guest ' : 'account '}links yet</Empty.Title
					>
					<Empty.Description>{owners.find((o) => o.value === owner)?.empty}</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{/snippet}
	</LinkList>
{/key}
