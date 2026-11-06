<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import { Button, buttonVariants } from '#lib/components/ui/button/index.js';
	import { cn } from '#lib/utils.ts';
	import AddressLabel from '#lib/components/AddressLabel.svelte';
	import Stamp from '#lib/components/Stamp.svelte';

	// Filled per request by the server (`Frontend::preview`), HTML-escaped.
	const slug = '%%TIINY_PREVIEW_SLUG%%';
	const url = '%%TIINY_PREVIEW_URL%%';
	const host = '%%TIINY_PREVIEW_HOST%%';
	const code = '%%TIINY_PREVIEW_CODE%%';

	// No hydration on status pages (it would bring back the placeholders), so Copy is a small inline script, shown only with a clipboard.
	const copyScript = `<script>{const b=document.querySelector("[data-copy]"),s=document.querySelector("[data-copy-status]");let t;if(b&&navigator.clipboard){b.hidden=false;b.onclick=()=>navigator.clipboard.writeText(b.dataset.copy).then(()=>{b.dataset.copied="";s.textContent="Copied";clearTimeout(t);t=setTimeout(()=>{delete b.dataset.copied;s.textContent=""},1600)},()=>{})}}<\/script>`;
	// CopyButton's crossfade: the outgoing layer fades fast, the incoming one waits for it.
	const out =
		'transition-opacity ease-out delay-100 duration-150 group-data-copied:opacity-0 group-data-copied:delay-0 group-data-copied:duration-100';
	const into =
		'opacity-0 transition-opacity ease-out duration-100 group-data-copied:opacity-100 group-data-copied:delay-100 group-data-copied:duration-150';
</script>

<svelte:head><title>{slug} goes to {host} · tiiny</title></svelte:head>

<div class="min-w-0">
	<h1 class="text-[clamp(40px,5vw,64px)] leading-[1.05]">
		This link goes to
		<span
			class="mt-3 block font-mono text-[clamp(24px,3.2vw,36px)] leading-tight font-bold [overflow-wrap:anywhere]"
			>{host}</span
		>
	</h1>
	<p class="mt-5 max-w-[44ch] text-[17px] text-muted-foreground">
		Check the full address before you continue.
	</p>
	<!-- Up to 2048 characters: capped and scrollable so the actions stay in view. -->
	<div
		class="mt-4 max-h-52 overflow-y-auto overscroll-contain rounded-lg border bg-card px-4 py-3 font-mono text-[15px] leading-relaxed break-all outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
	>
		{url}
	</div>
	<span class="sr-only" aria-live="polite" data-copy-status></span>
	<div class="mt-6 flex items-center gap-3">
		<Button href={url} rel="noopener" class="min-w-0 shrink"
			><span class="truncate">Continue to {host}</span></Button
		>
		<button
			type="button"
			hidden
			data-copy={url}
			aria-label="Copy address"
			class={cn(
				buttonVariants({ variant: 'secondary' }),
				'group shrink-0 data-copied:text-stamp max-sm:w-10 max-sm:px-0'
			)}
		>
			<span class="relative size-4 shrink-0" aria-hidden="true">
				<span class="absolute inset-0 grid place-items-center {out}"
					><CopyIcon class="size-4" /></span
				>
				<span class="absolute inset-0 grid place-items-center {into}"
					><CheckIcon class="size-4" /></span
				>
			</span>
			<span class="grid justify-items-start max-sm:hidden" aria-hidden="true">
				<span class="col-start-1 row-start-1 {out}">Copy address</span>
				<span class="col-start-1 row-start-1 {into}">Copied</span>
			</span>
		</button>
	</div>
	<a
		class="mt-4 inline-block text-[15px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
		href="/report?slug={slug}">Report this link</a
	>
</div>
<div aria-hidden="true">
	<AddressLabel class="rotate-[-1.5deg]">
		<p class="slug truncate text-[clamp(30px,9vw,40px)]"><span class="host">/</span>{slug}</p>
		<p class="mt-3 text-[13px] text-muted-foreground">Redirects to</p>
		<p class="mt-0.5 truncate font-mono text-[15px] text-muted-foreground">{host}</p>
		{#snippet stamp()}
			<Stamp label="Preview" value={code} rotate={-8} />
		{/snippet}
	</AddressLabel>
</div>
{@html copyScript}
