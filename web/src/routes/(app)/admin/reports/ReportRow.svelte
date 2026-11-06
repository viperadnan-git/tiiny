<script lang="ts">
	import RelativeTime from '#lib/components/RelativeTime.svelte';
	import { shortHost } from '#lib/origin.ts';
	import { toast } from 'svelte-sonner';
	import { api } from '#lib/api-client.ts';
	import { toastError } from '#lib/toast.ts';
	import Stamp from '#lib/components/Stamp.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { confirm } from '#lib/confirm.svelte.ts';
	import { formatNumber } from '#lib/format.ts';
	import { placeholderReport } from '#lib/placeholders.ts';
	import type { ReportAction, ReportRow } from '#lib/types.ts';

	let {
		report: shown,
		onresolved
	}: {
		/** `null` renders the loading state with the same layout. */
		report: ReportRow | null;
		onresolved: () => void;
	} = $props();

	const loading = $derived(shown === null);
	const r = $derived(shown ?? placeholderReport);
	const link = $derived(r.link);
	const others = $derived(r.open_for_link - 1);
	const outcome: Record<string, string> = {
		blocked: 'Blocked',
		deleted: 'Deleted',
		owner_disabled: 'Owner disabled'
	};
	let busy = $state(false);

	const reports = (n: number) => (n === 1 ? '1 report' : `${formatNumber(n)} reports`);
	const failed = $derived<Record<ReportAction, string>>({
		block: `Couldn't block /${r.slug}`,
		delete: `Couldn't delete /${r.slug}`,
		disable_owner: `Couldn't disable the owner of /${r.slug}`,
		dismiss: `Couldn't dismiss the report on /${r.slug}`
	});
	function succeeded(action: ReportAction) {
		const closed = `${reports(r.open_for_link)} closed.`;
		if (action === 'block') toast.success(`/${r.slug} blocked`, { description: closed });
		else if (action === 'delete') toast.success(`/${r.slug} deleted`, { description: closed });
		else if (action === 'disable_owner')
			toast.success(`${link?.owner_email} disabled`, {
				description: `/${r.slug} is blocked. ${closed}`
			});
		else toast.success('Report dismissed', { description: `/${r.slug} keeps redirecting.` });
	}

	async function resolve(action: ReportAction) {
		if (action === 'block') {
			const ok = await confirm({
				title: `Block /${r.slug}?`,
				description: `Visitors get a “disabled for abuse” page instead of the destination, and ${r.open_for_link === 1 ? 'this report is' : `all ${r.open_for_link} open reports on it are`} closed. You can unblock it from the link later.`,
				action: 'Block link',
				cancel: 'Keep active',
				destructive: true
			});
			if (!ok) return;
		}
		if (action === 'delete') {
			const ok = await confirm({
				title: `Delete /${r.slug}?`,
				description: `The link and its stats are deleted and ${r.open_for_link === 1 ? 'this report is' : `all ${r.open_for_link} open reports on it are`} closed. A permanent slug is retired. This can't be undone.`,
				action: 'Delete link',
				cancel: 'Keep link',
				destructive: true
			});
			if (!ok) return;
		}
		if (action === 'disable_owner') {
			const ok = await confirm({
				title: `Disable ${link?.owner_email}?`,
				description:
					'They are signed out and all their links stop redirecting. This link is also blocked and its open reports are closed. You can enable them again from Users.',
				action: 'Disable owner',
				cancel: 'Keep enabled',
				destructive: true
			});
			if (!ok) return;
		}
		busy = true;
		try {
			await api('POST', `/admin/reports/${r.id}/resolve`, { action });
			succeeded(action);
			onresolved();
		} catch (e) {
			toastError(failed[action], e);
		} finally {
			busy = false;
		}
	}

	/** The inverse of what the outcome changed, when one exists: deleted links can't come back. */
	const undo = $derived(
		!link || r.action === 'deleted'
			? null
			: r.disabled_owner
				? {
						label: 'Undo',
						ask: {
							title: `Enable ${link.owner_email} again?`,
							description: `They can sign in again and their links redirect.${r.blocked_link ? ` /${r.slug} is unblocked, and the` : ' The'} reports closed with it go back to Open.`,
							action: 'Enable and reopen',
							cancel: 'Keep disabled'
						}
					}
				: r.blocked_link
					? {
							label: 'Undo block',
							ask: {
								title: `Unblock /${r.slug}?`,
								description:
									'It redirects again, and the reports closed when it was blocked go back to Open.',
								action: 'Unblock and reopen',
								cancel: 'Keep blocked'
							}
						}
					: // Dismissed, or actioned on something already blocked/disabled: nothing to undo but the status.
						{ label: 'Reopen' }
	);

	async function reopen() {
		if (!undo || (undo.ask && !(await confirm(undo.ask)))) return;
		busy = true;
		try {
			const { reopened } = await api<{ reopened: number }>(
				'POST',
				`/admin/reports/${r.id}/reopen`,
				{}
			);
			if (r.status === 'dismissed')
				toast.success('Report reopened', { description: `/${r.slug} is back in Open.` });
			else if (r.action === 'owner_disabled')
				toast.success(`${link?.owner_email} enabled`, {
					description: `/${r.slug} is unblocked. ${reports(reopened)} reopened.`
				});
			else toast.success(`/${r.slug} unblocked`, { description: `${reports(reopened)} reopened.` });
			onresolved();
		} catch (e) {
			toastError(`Couldn't reopen the report on /${r.slug}`, e);
		} finally {
			busy = false;
		}
	}
</script>

<li>
	<!-- Address label with the stamp in the corner; details and actions span the full width below. -->
	<Card.Root class="gap-0 rounded-lg px-5 py-5 shadow-paper sm:px-7 sm:py-6">
		<div class="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-x-3 sm:gap-x-5">
			<!-- Phones: the stamp sits beside the reason line only, so the address gets the full width. -->
			<div class="min-w-0 self-center sm:self-start">
				<div class="flex flex-wrap items-center gap-x-2.5 gap-y-1.5">
					<Skeleton {loading} class="rounded-[5px]"
						><Badge variant="stamp-destructive">{r.reason}</Badge></Skeleton
					>
					<span class="font-mono text-[13px] text-muted-foreground"
						><Skeleton {loading} class="inline-block"
							>filed <RelativeTime unix={r.created_at} /></Skeleton
						></span
					>
					{#if link?.blocked && r.status === 'open'}<Badge variant="tag">already blocked</Badge
						>{/if}
				</div>
			</div>
			<div class="col-span-2 row-start-2 min-w-0 sm:col-span-1">
				<a
					href={loading || !link ? undefined : `/dashboard/links/${link.slug}`}
					class="slug mt-3 block min-w-0 text-[clamp(20px,2.4vw,26px)] hover:text-stamp"
				>
					<span class="host block pb-1 text-[0.7em] sm:inline sm:p-0 sm:text-[1em]"
						><Skeleton {loading} class="inline-block">{shortHost}/</Skeleton></span
					><Skeleton {loading} class="inline-block">{r.slug}</Skeleton>
				</a>
				<p
					class="mt-1 truncate font-mono text-[14px] text-muted-foreground"
					title={loading ? undefined : r.url}
				>
					<Skeleton {loading} class="inline-block max-w-full overflow-hidden">{r.url}</Skeleton>
				</p>
				<p class="mt-1 text-[14px] wrap-anywhere text-muted-foreground">
					<Skeleton {loading} class="inline-block max-w-full overflow-hidden">
						{#if !link}Link since deleted{:else if link.owner_email}Owned by <span
								class="text-foreground">{link.owner_email}</span
							>{:else}Guest link, no owner{/if}
					</Skeleton>
				</p>
			</div>
			<div class="col-start-2 row-start-1 pt-1 sm:row-span-2">
				<!-- No stamp once the link is gone: there's nothing left to count reports against. -->
				{#if r.status === 'open' && (link || loading)}
					<Skeleton {loading} class="rotate-3 rounded-lg">
						<Stamp
							label={r.open_for_link === 1 ? 'Report' : 'Reports'}
							value={r.open_for_link}
							rotate={loading ? 0 : 3}
						/>
					</Skeleton>
				{:else if r.status !== 'open'}
					<Badge variant="seal" class="py-3 text-[12px] sm:py-3.5" style="rotate: 3deg">
						{r.status === 'dismissed' ? 'Dismissed' : (outcome[r.action ?? ''] ?? 'Actioned')}
					</Badge>
				{/if}
			</div>
		</div>

		{#if r.details}
			<blockquote
				class="mt-4 max-w-[65ch] border-l-2 border-stamp/60 pl-3.5 font-serif text-[16px] leading-relaxed wrap-break-word whitespace-pre-line"
			>
				<Skeleton {loading} class="inline-block">{r.details}</Skeleton>
			</blockquote>
		{/if}
		{#if r.contact_email}
			<p class="mt-2.5 truncate text-[14px] text-muted-foreground">
				<Skeleton {loading} class="inline-block max-w-full overflow-hidden"
					>Reply to <a
						class="font-mono text-foreground hover:text-stamp"
						href="mailto:{r.contact_email}">{r.contact_email}</a
					></Skeleton
				>
			</p>
		{/if}

		{#if r.status === 'open'}
			<Card.Footer class="mt-5 block rounded-none border-t-2 border-dashed border-border px-0 pt-4">
				<div class="flex flex-wrap gap-2" role="group" aria-label="Actions for report on /{r.slug}">
					<Skeleton {loading} class="rounded-full"
						><Button size="sm" disabled={busy || loading || !link} onclick={() => resolve('block')}
							>Block link</Button
						></Skeleton
					>
					<Skeleton {loading} class="rounded-full"
						><Button
							size="sm"
							variant="destructive-secondary"
							disabled={busy || loading || !link}
							onclick={() => resolve('delete')}>Delete link</Button
						></Skeleton
					>
					<Skeleton {loading} class="rounded-full">
						<Button
							size="sm"
							variant="secondary"
							disabled={busy || loading || !link?.owner_email}
							title={link && !link.owner_email ? 'Guest links have no owner' : undefined}
							onclick={() => resolve('disable_owner')}>Disable owner</Button
						>
					</Skeleton>
					<Skeleton {loading} class="rounded-full"
						><Button
							size="sm"
							variant="ghost"
							disabled={busy || loading}
							onclick={() => resolve('dismiss')}>Dismiss</Button
						></Skeleton
					>
				</div>
				{#if others > 0}
					<p class="mt-2.5 text-[13px] text-muted-foreground">
						Block, delete and disable also close the {formatNumber(others)} other open {others === 1
							? 'report'
							: 'reports'} on this link.
					</p>
				{/if}
			</Card.Footer>
		{:else if r.resolved_at}
			<Card.Footer
				class="mt-4 flex-wrap justify-between gap-x-4 gap-y-2 rounded-none border-t-2 border-dashed border-border px-0 pt-3"
			>
				<div>
					<p class="font-mono text-[13px] text-muted-foreground">
						{r.status === 'dismissed' ? 'Dismissed' : 'Actioned'}
						<RelativeTime unix={r.resolved_at} />
					</p>
					{#if r.action === 'deleted'}<p class="mt-0.5 text-[13px] text-muted-foreground">
							Deleted links can't be restored.
						</p>{/if}
				</div>
				{#if undo}
					<Button size="sm" variant="secondary" disabled={busy} onclick={reopen}
						>{undo.label}</Button
					>
				{/if}
			</Card.Footer>
		{/if}
	</Card.Root>
</li>
