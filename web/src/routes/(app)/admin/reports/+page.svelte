<script lang="ts">
	import { page } from '$app/state';
	import { api, qs } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import ListHeader from '#lib/components/ListHeader.svelte';
	import ListStatus from '#lib/components/ListStatus.svelte';
	import ReportRow from './ReportRow.svelte';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import * as Tabs from '#lib/components/ui/tabs/index.js';
	import { paged } from '#lib/paged.svelte.ts';
	import { SKELETON_ROWS } from '#lib/placeholders.ts';
	import { refreshOpenReports } from '#lib/report-count.svelte.ts';
	import type { ReportPage, ReportRow as Report, ReportStatus } from '#lib/types.ts';

	const statuses: { value: ReportStatus; label: string; empty: string }[] = [
		{
			value: 'open',
			label: 'Open',
			empty: 'Nothing to review. New reports from /report land here.'
		},
		{
			value: 'actioned',
			label: 'Actioned',
			empty: 'Reports closed by blocking, deleting or disabling show here.'
		},
		{ value: 'dismissed', label: 'Dismissed', empty: 'Reports you dismiss show here.' }
	];
	let status = $state<ReportStatus>('open');
	const list = paged<Report>(async (query, cursor) => {
		const res = await api<ReportPage>('GET', `/admin/reports${qs({ status, query, cursor })}`);
		return { items: res.reports, next_cursor: res.next_cursor };
	});
	// `?q=` opens the page already searched, e.g. from a link's "View reports".
	list.search = page.url.searchParams.get('q') ?? '';

	function resolved() {
		void list.load(false);
		void refreshOpenReports();
	}
</script>

<svelte:head><title>Reports · Admin · tiiny</title></svelte:head>

<div class="grid gap-6">
	<ListHeader
		title="Abuse reports"
		search={{
			id: 'report-search',
			label: 'Search reports',
			placeholder: 'Search by slug, URL or email'
		}}
		bind:value={list.search}
	/>

	<!-- Manual activation: arrows move focus, Enter/Space switches (each switch refetches). -->
	<Tabs.Root
		bind:value={status}
		onValueChange={() => list.reset()}
		activationMode="manual"
		class="gap-6"
	>
		<Tabs.List aria-label="Report status" class="max-w-full overflow-x-auto">
			{#each statuses as s (s.value)}
				<Tabs.Trigger value={s.value}>{s.label}</Tabs.Trigger>
			{/each}
		</Tabs.List>
		<Tabs.Content value={status} class="grid gap-6 text-base">
			<FormAlert message={list.error} />

			{#if list.items.length || list.skeleton}
				<ul class="grid gap-5" aria-busy={list.loading}>
					{#each list.rows(Array<null>(SKELETON_ROWS).fill(null)) as report, i (report?.id ?? `s${i}`)}
						{#if report}<ReportRow {report} onresolved={resolved} />{:else}<ReportRow
								report={null}
								onresolved={() => {}}
							/>{/if}
					{/each}
				</ul>
			{/if}

			<ListStatus
				{list}
				hint="Search looks at slugs, destinations, details, contact and owner addresses."
			>
				{#snippet empty()}
					<Empty.Root class="border-border">
						<Empty.Header>
							<Empty.Title>No {status} reports</Empty.Title>
							<Empty.Description
								>{statuses.find((s) => s.value === status)?.empty}</Empty.Description
							>
						</Empty.Header>
					</Empty.Root>
				{/snippet}
			</ListStatus>
		</Tabs.Content>
	</Tabs.Root>
</div>
