<script lang="ts">
	import { untrack } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { api } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Dialog from '#lib/components/ui/dialog/index.js';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { spaForm } from '#lib/forms.ts';
	import { formatNumber } from '#lib/format.ts';
	import { shortHost } from '#lib/origin.ts';
	import SlugInput from '#lib/components/SlugInput.svelte';
	import { updateLinkFormSchema, urlMax, type UpdateLinkBody } from '#lib/schemas.ts';
	import type { Link } from '#lib/types.ts';

	let { link = $bindable(), onsaved }: { link: Link | null; onsaved: (l: Link) => void } = $props();

	let current = $state<Link | null>(null);
	const host = $derived(current ? shortHost : '');

	const form = spaForm(
		updateLinkFormSchema,
		{ url: '', slug: '' },
		async (data) => {
			if (!current) return;
			const changes: UpdateLinkBody = {};
			if (data.url !== current.url) changes.url = data.url;
			if (data.slug !== current.slug) changes.slug = data.slug;
			const old = current.slug;
			if (Object.keys(changes).length) onsaved(await api<Link>('PATCH', `/links/${old}`, changes));
			link = null;
			const slug = changes.slug ?? old;
			toast.success(
				`/${slug} saved`,
				slug !== old ? { description: `/${old} no longer works.` } : undefined
			);
		},
		{ resetForm: false }
	);
	const { form: formData, enhance, message, submitting } = form;
	const slugChanged = $derived(!!current && $formData.slug !== current.slug);
	const urlChanged = $derived(!!current && $formData.url !== current.url);

	$effect(() => {
		const l = link;
		if (!l) return;
		untrack(() => {
			current = l;
			form.reset({ data: { url: l.url, slug: l.slug } });
		});
	});
</script>

<!-- Permanent links can't be edited (the server answers 409), so the 302 copy below always holds. Blocked links are read-only (403). -->
<!-- Who may edit (owners not on blocked links, admins always) is decided by the menu and the server. -->
<Dialog.Root open={!!link && !link.permanent} onOpenChange={(o) => !o && (link = null)}>
	<Dialog.Content class="gap-0 overflow-hidden p-0 sm:max-w-lg">
		<Dialog.Header class="gap-3 px-7 pt-7 pb-5">
			<div class="flex items-center gap-2 pr-10">
				<Dialog.Title>Edit link</Dialog.Title>
				<Badge variant="tag">temporary</Badge>
			</div>
			<!-- Live preview of the short link as it will read after saving. -->
			<p class="slug text-[22px]" aria-hidden="true">
				<span class="host">{host}/</span><span class={slugChanged ? 'text-stamp' : ''}
					>{$formData.slug || '…'}</span
				>
			</p>
			<Dialog.Description>
				{current
					? `Redirects with 302. ${formatNumber(current.click_count)} ${current.click_count === 1 ? 'click' : 'clicks'} so far, kept when you change it.`
					: ''}
			</Dialog.Description>
		</Dialog.Header>
		<form method="POST" class="grid gap-5 border-t px-7 pt-6 pb-7" use:enhance>
			<Form.Field {form} name="url">
				<Form.Control>
					{#snippet children({ props })}
						<Form.Label>Destination</Form.Label>
						<Input
							{...props}
							type="url"
							required
							maxlength={urlMax}
							class="font-mono"
							bind:value={$formData.url}
						/>
					{/snippet}
				</Form.Control>
				{#if urlChanged && current}
					<Form.Description class="enter truncate font-mono" title={current.url}
						>Was {current.url}</Form.Description
					>
				{/if}
				<Form.FieldErrors errorClasses="enter" />
			</Form.Field>
			<Form.Field {form} name="slug">
				<Form.Control>
					{#snippet children({ props })}
						<Form.Label>Slug</Form.Label>
						<SlugInput {...props} required placeholder="abc123" bind:value={$formData.slug} />
					{/snippet}
				</Form.Control>
				{#if slugChanged && current}
					<Form.Description class="enter text-stamp">
						<span class="font-mono">{host}/{current.slug}</span> stops working when you save, and anyone
						can then claim it for their own link.
					</Form.Description>
				{/if}
				<Form.FieldErrors errorClasses="enter" />
			</Form.Field>
			<FormAlert message={$message} class="enter" />
			<Dialog.Footer class="pt-1">
				<Button variant="secondary" onclick={() => (link = null)}>Cancel</Button>
				<Form.Button disabled={$submitting || !(slugChanged || urlChanged)}
					>{$submitting ? 'Saving…' : 'Save changes'}</Form.Button
				>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
