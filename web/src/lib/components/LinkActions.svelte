<script lang="ts">
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import { goto } from '$app/navigation';
	import { toast } from 'svelte-sonner';
	import { api } from '#lib/api-client.ts';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as DropdownMenu from '#lib/components/ui/dropdown-menu/index.js';
	import { confirm } from '#lib/confirm.svelte.ts';
	import { toastError } from '#lib/toast.ts';
	import type { Link } from '#lib/types.ts';

	let {
		link,
		admin = false,
		disabled = false,
		onchange,
		onremove,
		onedit
	}: {
		link: Link;
		/** Adds Block/Unblock, View reports and View owner; Edit stays available on blocked links. */
		admin?: boolean;
		disabled?: boolean;
		onchange: (l: Link) => void;
		onremove: (id: number) => void;
		onedit: (l: Link) => void;
	} = $props();

	let busy = $state(false);

	async function run(failed: string, fn: () => Promise<void>) {
		busy = true;
		try {
			await fn();
		} catch (e) {
			toastError(failed, e);
		} finally {
			busy = false;
		}
	}

	async function makePermanent() {
		const ok = await confirm({
			title: `Make ${link.slug} permanent?`,
			description:
				"It will redirect with 301, so browsers may cache it. It can't be edited or made temporary again, and repeat visits from the same browser won't be counted.",
			action: 'Make permanent',
			cancel: 'Keep temporary'
		});
		if (!ok) return;
		await run(`Couldn't make /${link.slug} permanent`, async () => {
			onchange(await api<Link>('PATCH', `/links/${link.slug}`, { permanent: true }));
			toast.success(`/${link.slug} is now permanent`, {
				description: 'It redirects with 301 from now on.'
			});
		});
	}

	async function setBlocked(blocked: boolean) {
		if (
			blocked &&
			!(await confirm({
				title: `Block ${link.slug}?`,
				description:
					'Visitors get a “disabled for abuse” page instead of the destination, and its owner can’t edit it. You can unblock it later.',
				action: 'Block link',
				cancel: 'Keep active',
				destructive: true
			}))
		)
			return;
		await run(`Couldn't ${blocked ? 'block' : 'unblock'} /${link.slug}`, async () => {
			onchange(await api<Link>('PATCH', `/admin/links/${link.slug}`, { blocked }));
			if (blocked)
				toast.success(`/${link.slug} blocked`, {
					description: 'Visitors see the “disabled for abuse” page.'
				});
			else toast.success(`/${link.slug} unblocked`, { description: 'It redirects again.' });
		});
	}

	async function remove() {
		const ok = await confirm({
			title: `Delete ${link.slug}?`,
			description:
				link.permanent || link.blocked
					? `This link is ${link.permanent ? 'permanent' : 'blocked'}, so the slug is retired: it will show “gone” and can't be reused unless an admin releases it. Its stats are deleted too.`
					: "Its stats are deleted too. This can't be undone.",
			action: 'Delete link',
			cancel: 'Keep link',
			destructive: true
		});
		if (!ok) return;
		const retired = link.permanent || link.blocked;
		await run(`Couldn't delete /${link.slug}`, async () => {
			await api('DELETE', `/links/${link.slug}`);
			onremove(link.id);
			toast.success(
				`/${link.slug} deleted`,
				retired ? { description: 'The slug is retired and shows “gone”.' } : undefined
			);
		});
	}
</script>

<!-- The "More actions" menu for one link: owner actions, plus admin ones. -->
<DropdownMenu.Root>
	<DropdownMenu.Trigger disabled={busy || disabled}>
		{#snippet child({ props })}
			<Button {...props} size="icon-sm" variant="ghost" aria-label="More actions for {link.slug}"
				><EllipsisIcon /></Button
			>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="start">
		{#if link.blocked && !admin}
			<DropdownMenu.Label class="text-xs font-normal text-muted-foreground"
				>Blocked by an admin</DropdownMenu.Label
			>
		{/if}
		{#if !link.permanent}
			<DropdownMenu.Item disabled={link.blocked && !admin} onSelect={() => onedit(link)}
				>Edit</DropdownMenu.Item
			>
			{#if link.user_id !== null}
				<DropdownMenu.Item disabled={link.blocked} onSelect={makePermanent}
					>Make permanent</DropdownMenu.Item
				>
			{/if}
			<DropdownMenu.Separator />
		{/if}
		{#if admin}
			<DropdownMenu.Item onSelect={() => setBlocked(!link.blocked)}
				>{link.blocked ? 'Unblock' : 'Block'}</DropdownMenu.Item
			>
			<DropdownMenu.Item onSelect={() => goto(`/admin/reports?q=${encodeURIComponent(link.slug)}`)}
				>View reports</DropdownMenu.Item
			>
			{#if link.user_id !== null}
				<DropdownMenu.Item onSelect={() => goto(`/admin/users/${link.user_id}`)}
					>View owner</DropdownMenu.Item
				>
			{/if}
			<DropdownMenu.Separator />
		{/if}
		<DropdownMenu.Item variant="destructive" onSelect={remove}>Delete</DropdownMenu.Item>
	</DropdownMenu.Content>
</DropdownMenu.Root>
