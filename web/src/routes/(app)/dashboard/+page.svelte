<script lang="ts">
	import { afterNavigate, invalidate, replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { tick } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { api, ApiError } from '#lib/api-client.ts';
	import { toastError } from '#lib/toast.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import * as Empty from '#lib/components/ui/empty/index.js';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import LinkList from '#lib/components/LinkList.svelte';
	import { copyText } from '#lib/clipboard.ts';
	import CopyButton from '#lib/components/CopyButton.svelte';
	import Stamp from '#lib/components/Stamp.svelte';
	import ClipboardIcon from '@lucide/svelte/icons/clipboard-paste';
	import { shortHost, shortUrl } from '#lib/origin.ts';
	import { spaForm } from '#lib/forms.ts';
	import { slugInput } from '#lib/slug-input.ts';
	import { createLinkFormSchema, urlMax } from '#lib/schemas.ts';
	import type { Link } from '#lib/types.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const host = shortHost;
	const needsVerify = $derived(
		data.providers.email_verification && !data.me.email_verified && data.me.role !== 'admin'
	);

	// Wraps after the @ on narrow screens instead of mid-word.
	const emailLocal = $derived(data.me.email.slice(0, data.me.email.lastIndexOf('@') + 1));
	const emailDomain = $derived(data.me.email.slice(data.me.email.lastIndexOf('@') + 1));

	let links = $state<Link[]>([]);
	let fresh = $state<number | null>(null);
	let resend = $state<'idle' | 'sending' | 'sent'>('idle');
	let urlInput = $state<HTMLInputElement | null>(null);
	let created = $state<{ link: Link; copied: boolean } | null>(null);
	let copyButton = $state<HTMLButtonElement | null>(null);

	const form = spaForm(
		createLinkFormSchema,
		{ url: '', slug: '' },
		async (body) => {
			const link = await api<Link>('POST', '/links', { ...body, slug: body.slug || undefined });
			links = [link, ...links];
			fresh = link.id;
			// Clipboard writes can be refused without a fresh user gesture; the Copy button covers that.
			created = { link, copied: await copyText(shortUrl(link.slug)) };
			// Copying is the next step after creating.
			await tick();
			copyButton?.focus();
		},
		{
			// A stale result must not sit beside a new error.
			onSubmit: () => {
				created = null;
			}
		}
	);
	const { form: formData, enhance, message, submitting } = form;

	/** `?url=` from the landing page: prefill once, then drop it so a reload doesn't refill. Runs after the router's focus reset. */
	afterNavigate(() => {
		const url = page.url.searchParams.get('url');
		if (url === null) return;
		$formData.url = url.slice(0, urlMax);
		urlInput?.focus();
		const clean = new URL(page.url.href);
		clean.searchParams.delete('url');
		replaceState(clean, page.state);
	});

	/** Paste a URL anywhere outside a text field to start a new link. */
	function pasteAnywhere(e: ClipboardEvent) {
		const el = e.target as HTMLElement | null;
		if (
			el?.closest(
				'input, textarea, [contenteditable], [role=dialog], [role=alertdialog], [role=menu]'
			)
		)
			return;
		const text = e.clipboardData?.getData('text/plain').trim() ?? '';
		if (/\s/.test(text) || !createLinkFormSchema.shape.url.safeParse(text).success) return;
		e.preventDefault();
		$formData.url = text;
		created = null;
		urlInput?.focus();
	}

	const canPaste = typeof navigator.clipboard?.readText === 'function';

	async function pasteFromClipboard() {
		try {
			$formData.url = (await navigator.clipboard.readText()).trim().slice(0, urlMax);
		} catch {
			// Refused or empty: the field still takes a normal paste.
		}
		urlInput?.focus();
	}

	async function resendVerification() {
		resend = 'sending';
		try {
			await api('POST', '/auth/resend-verification');
			resend = 'sent';
			toast.success('Verification email sent', {
				description: `Open the link in the email sent to ${data.me.email}.`
			});
		} catch (err) {
			resend = 'idle';
			// Confirmed elsewhere (another tab, an admin, a password reset): refresh `me` to drop the banner.
			if (err instanceof ApiError && err.status === 409) {
				toast.success('Your email is already verified');
				await invalidate('app:auth');
			} else toastError("Couldn't send the verification email", err);
		}
	}
</script>

<svelte:head><title>Links · tiiny</title></svelte:head>
<svelte:window onpaste={pasteAnywhere} />

<div class="grid gap-14">
	{#if needsVerify}
		<!-- A letter in transit: airmail edge, the user's address on ruled lines, postage that lands once resent. -->
		<Card.Root
			aria-labelledby="verify-heading"
			role="region"
			class="gap-0 rounded-xl py-0 shadow-paper"
		>
			<div
				class="h-2 bg-[repeating-linear-gradient(135deg,var(--stamp)_0_14px,transparent_14px_28px,var(--destructive)_28px_42px,transparent_42px_56px)]"
				aria-hidden="true"
			></div>
			<div class="grid gap-5 p-5 sm:p-7 md:grid-cols-[minmax(0,1fr)_minmax(0,22rem)] md:gap-x-10">
				<div class="min-w-0 md:self-end">
					<h2 id="verify-heading" class="text-[clamp(24px,3vw,30px)]">
						Verify your email to create links
					</h2>
					<p class="mt-2 max-w-[48ch] text-muted-foreground">
						Open the link in the email we sent you. It can take a minute to arrive, so check spam
						too.
					</p>
				</div>
				<div
					class="flex items-start justify-between gap-4 rounded-lg border border-dashed border-border p-4 sm:px-5 md:col-start-2 md:row-span-2 md:row-start-1 md:self-center md:py-5"
				>
					<div class="min-w-0 flex-1">
						<p class="text-[13px] text-muted-foreground">Sent to</p>
						<div
							class="mt-1 bg-[repeating-linear-gradient(transparent,transparent_27px,var(--rule)_27px,var(--rule)_28px)] font-mono text-[15px] leading-7"
						>
							<p class="font-bold wrap-anywhere">{emailLocal}<wbr />{emailDomain}</p>
							<p aria-hidden="true">&nbsp;</p>
						</div>
					</div>
					<div class="shrink-0" aria-hidden="true">
						{#if resend === 'sent'}
							<Stamp label="Sent" value="Again" rotate={-9} land />
						{:else}
							<span
								class="block h-15.5 w-21 rotate-[-9deg] rounded-lg border-[2.5px] border-dashed border-border sm:h-18 sm:w-26"
							></span>
						{/if}
					</div>
				</div>
				<div class="min-h-9 md:self-start">
					{#if resend === 'sent'}
						<p role="status" class="enter pt-1.5 text-sm text-muted-foreground">
							Sent again. Check your inbox.
						</p>
					{:else}
						<Button
							variant="secondary"
							class="w-full sm:w-auto"
							onclick={resendVerification}
							disabled={resend === 'sending'}>Resend verification email</Button
						>
					{/if}
				</div>
			</div>
		</Card.Root>
	{/if}

	<section id="new" aria-labelledby="create-heading" class="grid scroll-mt-24 gap-4 sm:gap-5">
		<div class="flex flex-wrap items-end justify-between gap-x-6 gap-y-1">
			<h1 id="create-heading" class="text-[clamp(32px,4.4vw,48px)]">Shorten a link</h1>
			<p class="hidden pb-1.5 text-sm text-muted-foreground md:block">
				Paste a link anywhere on this page to start.
			</p>
		</div>
		<!-- A routing slip: where it goes, the short address it gets, and its postage (redirect type). -->
		<Card.Root class="gap-0 py-0 shadow-paper">
			<form method="POST" use:enhance>
				<Form.Field {form} name="url" class="gap-0">
					<div
						class="flex items-center gap-3 border-b-2 border-dashed border-border px-4 transition-colors focus-within:border-stamp has-aria-invalid:border-destructive sm:gap-4 sm:px-7"
					>
						<Form.Control>
							{#snippet children({ props })}
								<Form.Label class="sr-only">Long URL</Form.Label>
								<Input
									{...props}
									bind:ref={urlInput}
									type="text"
									inputmode="url"
									enterkeyhint="go"
									required
									maxlength={urlMax}
									placeholder={created ? 'Paste another URL' : 'Paste a long URL'}
									autocomplete="off"
									autocapitalize="off"
									spellcheck={false}
									class="h-14 min-w-0 flex-1 rounded-none border-0 bg-transparent px-0 font-mono text-base shadow-none focus-visible:ring-0 sm:h-16 md:text-[17px] dark:bg-transparent"
									oninput={() => (created = null)}
									bind:value={$formData.url}
								/>
							{/snippet}
						</Form.Control>
						{#if canPaste && !$formData.url}
							<Button
								type="button"
								variant="secondary"
								size="sm"
								class="shrink-0"
								onclick={pasteFromClipboard}
							>
								<ClipboardIcon />Paste
							</Button>
						{/if}
					</div>
					<Form.FieldErrors class="px-4 pt-2 sm:px-7" />
				</Form.Field>

				<div
					class="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-4 py-5 sm:gap-6 sm:px-7 sm:py-7"
				>
					{#if created}
						<div class="enter min-w-0" role="status">
							<p class="text-[13px] text-muted-foreground">
								{created.copied ? 'Created and copied' : 'Created'}
							</p>
							<a
								href={shortUrl(created.link.slug)}
								target="_blank"
								rel="noopener"
								class="slug mt-1 block text-[clamp(24px,4.6vw,40px)] hover:text-stamp"
							>
								<span class="host block pb-1 text-[0.6em] sm:inline sm:p-0 sm:text-[1em]"
									>{shortHost}/</span
								>{created.link.slug}
							</a>
							<p
								class="mt-1.5 truncate font-mono text-[13px] text-muted-foreground"
								title={created.link.url}
							>
								{created.link.url}
							</p>
							<div class="mt-4 flex flex-wrap items-center gap-2">
								<CopyButton
									bind:ref={copyButton}
									value={shortUrl(created.link.slug)}
									bind:copied={created.copied}
									resetMs={0}
									size="default"
									variant={created.copied ? 'secondary' : 'default'}
								/>
								<Button variant="ghost" href="/dashboard/links/{created.link.slug}">Stats</Button>
							</div>
						</div>
					{:else}
						<Form.Field {form} name="slug" class="min-w-0 gap-0">
							<Form.Control>
								{#snippet children({ props })}
									<Form.Label class="text-[13px] font-normal text-muted-foreground"
										>Short link</Form.Label
									>
									<div
										class="slug mt-1 flex flex-wrap items-baseline text-[clamp(24px,4.6vw,40px)] sm:flex-nowrap"
									>
										<span
											class="host w-full shrink-0 pb-1 text-[0.6em] sm:w-auto sm:p-0 sm:text-[1em]"
											>{host}/</span
										>
										<Input
											{...props}
											placeholder="random"
											autocomplete="off"
											autocapitalize="off"
											spellcheck={false}
											enterkeyhint="go"
											class="h-auto min-w-0 flex-[1_1_8ch] rounded-none border-0 border-b-2 border-dashed border-border bg-transparent px-0 py-0 font-mono text-[length:inherit] font-bold shadow-none placeholder:font-normal placeholder:text-muted-foreground/50 focus-visible:border-stamp focus-visible:ring-0 aria-invalid:border-destructive aria-invalid:ring-0 md:text-[length:inherit] dark:bg-transparent"
											bind:value={$formData.slug}
											{...slugInput((v) => ($formData.slug = v))}
										/>
									</div>
								{/snippet}
							</Form.Control>
							<Form.FieldErrors class="pt-2" />
						</Form.Field>
					{/if}
					<!-- Postage slot: shows the redirect code the link will carry, stamped once it exists. -->
					<div class="grid w-21 place-items-center self-start pt-1 sm:w-26">
						{#if created}
							<Stamp label={created.copied ? 'Copied' : 'Ready'} value="302" rotate={-8} land />
						{:else}
							<span
								class="grid h-15.5 w-full rotate-[-8deg] place-items-center rounded-lg border-[2.5px] border-dashed border-border font-mono text-[19px] font-bold text-muted-foreground/60 sm:h-18 sm:text-[26px]"
								aria-hidden="true">302</span
							>
						{/if}
					</div>
				</div>

				<Card.Footer
					class="grid gap-3 rounded-none border-t bg-secondary/60 px-4 py-4 sm:px-7 sm:py-5 [.border-t]:pt-4 sm:[.border-t]:pt-5"
				>
					<div class="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between sm:gap-8">
						<p class="max-w-[60ch] text-sm text-muted-foreground">
							Links start as temporary 302 redirects. To make one permanent, use
							<span class="font-medium text-foreground">Make permanent</span> in its menu.
						</p>
						<Form.Button
							size="lg"
							class="h-12 w-full px-8 text-base sm:w-auto"
							disabled={$submitting}>{$submitting ? 'Shortening…' : 'Shorten'}</Form.Button
						>
					</div>
					<FormAlert message={$message} />
				</Card.Footer>
			</form>
		</Card.Root>
	</section>

	<LinkList endpoint="/links" bind:links {fresh}>
		{#snippet empty()}
			<Empty.Root class="border-border">
				<Empty.Header>
					<Empty.Title>No links yet</Empty.Title>
					<Empty.Description>Paste a long URL above and press Shorten.</Empty.Description>
				</Empty.Header>
				<Empty.Content
					><Button variant="secondary" onclick={() => urlInput?.focus()}>Paste a URL</Button
					></Empty.Content
				>
			</Empty.Root>
		{/snippet}
	</LinkList>
</div>
