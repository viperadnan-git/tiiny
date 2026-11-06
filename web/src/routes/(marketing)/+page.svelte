<script lang="ts">
	import { withNext } from '#lib/safe-next.ts';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { api, errorMessage, getMe, sentence } from '#lib/api-client.ts';
	import { copyText } from '#lib/clipboard.ts';
	import GuestLinks, { GUEST_HINT_KEY } from './GuestLinks.svelte';
	import type { Link, Providers } from '#lib/types.ts';
	import * as Accordion from '#lib/components/ui/accordion/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import * as InputGroup from '#lib/components/ui/input-group/index.js';
	import Seo from '#lib/components/Seo.svelte';
	import GithubRepoCard from './GithubRepoCard.svelte';
	import PaperGrain from './PaperGrain.svelte';
	import Stamp from '#lib/components/Stamp.svelte';
	import { faqLd, faqs, softwareLd } from './content.ts';
	import { createLinkFormSchema, urlMax } from '#lib/schemas.ts';
	import { readmeUrl, repoUrl, version, siteDescription } from '#lib/site.ts';
	import { shortHost, shortUrl } from '#lib/origin.ts';

	// What the redirect handler writes: daily counts per dimension (src/stats), nothing per visit.
	const kept = [
		'The day',
		'Referring site, host only',
		'Country',
		'Device type',
		'Browser family',
		'Operating system family'
	];
	const never = [
		'IP address',
		'User-agent string',
		'Full referrer URL',
		'Cookies',
		'A row per visit'
	];

	// From `cargo run --release --example bench` (examples/bench.rs) on an Apple M1 (macOS arm64).
	const latency = [
		{ label: 'From memory', ms: 0.008, note: 'median, p99 0.01 ms' },
		{ label: 'From the database', ms: 0.4, note: 'median first hit, before it’s cached' }
	];
	const footprint = [
		{ label: 'Ready after start', value: '0.21 s', note: 'existing database; first boot 0.49 s' },
		{ label: 'Memory at idle', value: '9 MB', note: 'physical footprint, macOS arm64 build' },
		{ label: 'Binary size', value: '28 MB', note: 'web app included, macOS arm64 build' },
		{ label: 'Cookies on a redirect', value: 'none', note: 'visitors are never tagged' }
	];

	let longUrl = $state('');
	let urlError = $state('');

	type Visitor = { signedIn: boolean; guestLinks: boolean };
	let visitor: Promise<Visitor> | undefined;
	let signedIn = $state<boolean | null>(null);
	let mounted = $state(false);
	let busy = $state(false);
	let created = $state<{ link: Link; copied: boolean } | null>(null);
	let guestList = $state<GuestLinks>();

	function probe(): Promise<Visitor> {
		const p = Promise.all([getMe().then(Boolean), api<Providers>('GET', '/auth/providers')]).then(
			([s, p]) => {
				signedIn = s;
				// Signing up claimed the guest links; drop GuestLinks' skeleton hint.
				if (s) {
					try {
						localStorage.removeItem(GUEST_HINT_KEY);
					} catch {
						// Storage blocked.
					}
				}
				return { signedIn: s, guestLinks: p.guest_links };
			}
		);
		p.catch(() => {});
		return p;
	}

	// Prerendered: who is visiting is only known in the browser.
	onMount(() => {
		mounted = true;
		visitor = probe();
	});

	/** Guests get a link here; signed-in visitors go to the dashboard; without guest links, sign-up carries the URL through `next`. */
	async function shorten(e: SubmitEvent) {
		e.preventDefault();
		if (busy) return;
		const parsed = createLinkFormSchema.shape.url.safeParse(longUrl);
		// Chrome's URL parser percent-encodes spaces in a host; the server rejects them.
		if (!parsed.success || /\s/.test(longUrl.trim())) {
			urlError = sentence(
				parsed.success ? 'invalid url' : (parsed.error.issues[0]?.message ?? 'invalid url')
			);
			return;
		}
		urlError = '';
		created = null;
		busy = true;
		try {
			let v: Visitor;
			try {
				v = await (visitor ??= probe());
			} catch {
				// A failed probe must not stick; retry once on submit.
				v = await (visitor = probe());
			}
			const dashboard = `/dashboard?url=${encodeURIComponent(parsed.data)}`;
			if (v.signedIn) return void goto(dashboard);
			if (!v.guestLinks) return void goto(withNext('/signup', dashboard));
			const link = await api<Link>('POST', '/links', { url: parsed.data });
			longUrl = '';
			// Clipboard writes can be refused without a fresh user gesture; the Copy button covers that.
			created = { link, copied: await copyText(shortUrl(link.slug)) };
			await guestList?.add(link, created.copied);
		} catch (err) {
			urlError = errorMessage(err);
		} finally {
			busy = false;
		}
	}
</script>

{#snippet tileHeader(title: string, text: string)}
	<Card.Header class="mt-7 gap-0">
		<Card.Title><h3 class="text-[24px] leading-[1.15]">{title}</h3></Card.Title>
		<Card.Description class="mt-2">{text}</Card.Description>
	</Card.Header>
{/snippet}

<Seo
	title="tiiny: self-hosted link shortener that counts clicks, not people"
	description={siteDescription}
	path="/"
	jsonLd={[softwareLd, faqLd]}
/>

<PaperGrain class="-mt-16 border-b pt-16 sm:-mt-17 sm:pt-17">
	<section class="wrap pt-10 pb-14 sm:pt-14 md:pt-20 md:pb-20">
		<div class="grid gap-x-16 gap-y-8 lg:grid-cols-[minmax(0,1fr)_minmax(0,26rem)] lg:items-center">
			<div>
				<h1 class="max-w-[13ch] text-[clamp(44px,7vw,96px)] leading-[0.95] tracking-[-0.035em]">
					Short links that count clicks, not people.
				</h1>
				<p class="mt-6 max-w-[44ch] text-[17px] text-muted-foreground sm:text-[19px]">
					Self-hosted. Choose the name, share the link, and see where clicks come from without
					storing who made them.
				</p>
			</div>
			<!-- A mail-forwarding sticker: the product in the post office's words. Decorative; the form below does it for real. -->
			<figure
				aria-hidden="true"
				class="relative mr-6 mb-6 hidden -rotate-2 rounded-lg border bg-card px-7 py-6 shadow-paper lg:block"
			>
				<p class="text-[13px] text-muted-foreground">Moved from</p>
				<p class="mt-1 font-mono text-[14px] leading-relaxed text-muted-foreground">
					<span class="strike"
						>https://store.example.com/<wbr />collections/<wbr />autumn-2026/<wbr />jackets?<wbr
						/>utm_source=newsletter&<wbr />utm_medium=email</span
					>
				</p>
				<p class="mt-5 text-[13px] text-muted-foreground">Forward to</p>
				<p class="slug mt-1 text-[34px] leading-none">/autumn</p>
				<Stamp
					label="Clicks"
					value={4812}
					rotate={8}
					land
					delay={900}
					class="absolute -right-6 -bottom-6"
				/>
			</figure>
		</div>

		<!-- The ticket is the form: paste on the left, the stamp lands on the stub once the link exists. -->
		<form
			class="mt-9 drop-shadow-[0_12px_16px_rgba(26,36,33,.08)] md:mt-12 dark:drop-shadow-none"
			onsubmit={shorten}
			novalidate
		>
			<Card.Root
				class="relative gap-0 rounded-xl py-0 md:ticket md:grid md:grid-cols-[minmax(0,1fr)_var(--stub)] md:[--stub:184px]"
			>
				<div
					class="min-w-0 p-5 sm:p-7 md:border-r-2 md:border-dashed md:border-border md:px-9 md:py-8"
				>
					<!-- The host gives way before the slug does: it shrinks on phones and truncates, the slug never does. -->
					<p
						class="slug flex min-w-0 items-baseline pr-26 text-[clamp(22px,2.6vw,30px)] whitespace-nowrap sm:pr-32 md:pr-0"
					>
						{#if created}
							<a
								href={shortUrl(created.link.slug)}
								target="_blank"
								rel="noopener"
								class="flex min-w-0 items-baseline hover:text-stamp"
								><span class="host min-w-0 truncate text-[0.65em] sm:text-[1em]">{shortHost}/</span
								><span class="shrink-0">{created.link.slug}</span></a
							>
						{:else}
							<span class="shrink-0 font-normal text-muted-foreground/60">/your-link</span>
						{/if}
					</p>
					<label for="hero-url" class="mt-4 block text-[13px] text-muted-foreground"
						>Redirects to</label
					>
					<InputGroup.Root class="mt-1.5 h-auto rounded-full p-1 sm:p-1.5">
						<InputGroup.Input
							id="hero-url"
							type="text"
							inputmode="url"
							enterkeyhint="go"
							maxlength={urlMax}
							placeholder="Paste a long URL"
							autocomplete="off"
							autocapitalize="off"
							spellcheck={false}
							aria-invalid={urlError ? true : undefined}
							aria-describedby="hero-url-note"
							class="h-11 px-3 font-mono text-base sm:h-12 sm:px-4 md:text-[17px]"
							bind:value={longUrl}
							oninput={() => {
								urlError = '';
								created = null;
							}}
						/>
						<InputGroup.Addon align="inline-end" class="py-0 pr-0 has-[>button]:mr-0">
							<Button
								type="submit"
								size="lg"
								disabled={busy}
								class="h-11 px-5 font-sans text-base sm:h-12 sm:px-7">Shorten</Button
							>
						</InputGroup.Addon>
					</InputGroup.Root>
					<p
						id="hero-url-note"
						class="mt-3 min-h-lh px-1 text-sm {urlError
							? 'enter text-destructive'
							: 'text-muted-foreground'}"
						aria-live="polite"
					>
						{#if urlError}<CircleAlertIcon
								class="mr-1.5 inline size-3.5 -translate-y-px"
								aria-hidden="true"
							/>{urlError}{:else if created}{created.copied
								? 'Copied to your clipboard.'
								: 'Created. Copy it from your recent links.'}{:else}Free, no account needed. Have
							one? <a href="/login" class="text-link whitespace-nowrap">Sign in</a>{/if}
					</p>
				</div>
				<!-- Phones: the stamp sits in the label's corner; from md it gets the stub. Positioned out of flow, so it never moves the form. -->
				<div
					class="absolute top-4 right-4 sm:top-6 sm:right-6 md:static md:grid md:place-items-center md:p-4"
					aria-hidden="true"
				>
					{#if created}
						{#key created.link.id}<Stamp
								label={created.copied ? 'Copied' : 'Created'}
								value={created.link.permanent ? '301' : '302'}
								rotate={-8}
								land
							/>{/key}
					{:else}
						<Stamp label="Clicks" value={0} rotate={-8} class="opacity-30" />
					{/if}
				</div>
			</Card.Root>
		</form>

		{#if mounted && signedIn !== true}<GuestLinks
				bind:this={guestList}
				class="mt-10 max-w-3xl"
			/>{/if}
	</section>
</PaperGrain>

<div class="wrap">
	<section aria-labelledby="why" class="py-20 sm:py-22">
		<div class="flex flex-wrap items-end justify-between gap-x-10 gap-y-4">
			<h2 id="why" class="max-w-[16ch] text-[clamp(34px,4.4vw,52px)]">
				Everything a short link needs, nothing about who clicked.
			</h2>
			<p><a class="text-link" href="/features">See every feature</a></p>
		</div>

		<div class="mt-10 grid gap-4 md:grid-cols-2 lg:grid-cols-3">
			<!-- Privacy: the widest tile, because it is the reason tiiny exists. -->
			<Card.Root
				class="grid gap-8 rounded-3xl [--card-spacing:--spacing(6)] sm:grid-cols-[1fr_1fr] sm:[--card-spacing:--spacing(8)] md:col-span-2"
			>
				<Card.Header class="gap-0 sm:col-span-2">
					<Card.Title
						><h3 class="text-[26px] leading-[1.15] sm:text-[30px]">
							What a click leaves behind
						</h3></Card.Title
					>
					<Card.Description class="mt-2 max-w-[52ch]"
						>Each visit adds one to a few daily counts and is gone. There is no visit log to leak,
						export or delete.</Card.Description
					>
				</Card.Header>
				<Card.Content class="block">
					<p class="text-sm font-semibold">Counted per day</p>
					<ul class="mt-3 grid gap-2 font-mono text-[15px]">
						{#each kept as k (k)}<li class="flex items-baseline gap-2.5">
								<span
									class="size-2 shrink-0 -translate-y-px rounded-full bg-stamp"
									aria-hidden="true"
								></span>{k}
							</li>{/each}
					</ul>
				</Card.Content>
				<Card.Content class="block">
					<p class="text-sm font-semibold">Never stored</p>
					<ul class="mt-3 grid gap-2 font-mono text-[15px] text-muted-foreground">
						{#each never as n (n)}<li class="flex items-baseline gap-2.5">
								<span
									class="size-2 shrink-0 -translate-y-px rounded-full border border-current"
									aria-hidden="true"
								></span><s class="decoration-1">{n}</s>
							</li>{/each}
					</ul>
				</Card.Content>
			</Card.Root>

			<Card.Root
				class="block rounded-3xl border-primary bg-primary text-primary-foreground [--card-spacing:--spacing(6)] sm:[--card-spacing:--spacing(8)] md:col-span-2 lg:col-span-1 lg:row-span-2"
			>
				<Card.Header class="gap-0">
					<Card.Title
						><h3 class="text-[26px] leading-[1.15]">
							Small enough to forget it’s running
						</h3></Card.Title
					>
					<Card.Description class="mt-2 text-primary-foreground/70"
						>Popular links are answered from memory, in a fraction of a millisecond.</Card.Description
					>
				</Card.Header>
				<Card.Content class="block">
					<figure class="mt-7">
						<figcaption class="text-sm font-semibold">Server time per redirect</figcaption>
						<div class="mt-4 grid gap-5">
							{#each latency as l (l.label)}
								<div>
									<div class="flex items-baseline justify-between gap-4">
										<span>{l.label}</span>
										<span class="font-mono text-[17px] font-bold">{l.ms} ms</span>
									</div>
									<div class="mt-2 h-2.5 rounded-full bg-primary-foreground/15" aria-hidden="true">
										<div
											class="h-full min-w-2.5 rounded-full bg-primary-foreground"
											style:width="{l.ms * 100}%"
										></div>
									</div>
									<p class="mt-1.5 font-mono text-[13px] text-primary-foreground/60">{l.note}</p>
								</div>
							{/each}
						</div>
					</figure>
					<dl class="mt-7 grid border-t border-primary-foreground/20 pt-2">
						{#each footprint as r (r.label)}
							<div class="flex flex-wrap items-baseline gap-x-3 py-2.5">
								<dt class="flex min-w-0 flex-1 items-baseline gap-3">
									{r.label}<span
										class="min-w-4 flex-1 -translate-y-1 border-b border-dotted border-primary-foreground/30"
										aria-hidden="true"
									></span>
								</dt>
								<dd class="font-mono text-[17px] font-bold">{r.value}</dd>
								<dd class="basis-full font-mono text-[12px] text-primary-foreground/60">
									{r.note}
								</dd>
							</div>
						{/each}
					</dl>
					<p class="mt-4 text-[13px] text-primary-foreground/60">
						Release build on an Apple M1 (macOS arm64), local libSQL file with an existing database,
						measured over loopback.
					</p>
				</Card.Content>
			</Card.Root>

			<Card.Root
				class="gap-0 rounded-3xl [--card-spacing:--spacing(6)] sm:[--card-spacing:--spacing(8)]"
			>
				<Card.Content class="flex-row gap-4" aria-hidden="true">
					<Stamp label="Temporary" value="302" rotate={-6} />
					<Stamp label="Permanent" value="301" rotate={5} />
				</Card.Content>
				{@render tileHeader(
					'302 or 301, chosen per link',
					'Temporary links stay editable. Permanent ones answer 301, and deleting one retires its slug, so a printed link never points somewhere new.'
				)}
			</Card.Root>

			<Card.Root
				class="gap-0 rounded-3xl [--card-spacing:--spacing(6)] sm:[--card-spacing:--spacing(8)]"
			>
				<Card.Content class="items-start"
					><code class="rounded-xl bg-muted px-3.5 py-2.5 font-mono text-[14px]"
						>docker run viperadnan/tiiny</code
					></Card.Content
				>
				{@render tileHeader(
					'One container, nothing else to run',
					'Redirects, the API and this web app are one binary with an embedded libSQL database. Add a Turso primary later for a replicated copy.'
				)}
			</Card.Root>

			<Card.Root
				class="gap-0 rounded-3xl [--card-spacing:--spacing(6)] sm:[--card-spacing:--spacing(8)]"
			>
				<Card.Content class="block">
					<ul class="flex flex-wrap gap-2" aria-label="Sign-in methods">
						{#each ['Password', 'Google', 'GitHub'] as m (m)}<li>
								<Badge variant="outline" class="h-7 px-3 text-sm font-normal">{m}</Badge>
							</li>{/each}
					</ul>
				</Card.Content>
				{@render tileHeader(
					'Accounts for a whole team',
					'An admin panel, a sign-up switch and optional daily link limits per user. Without SMTP, the operator resets passwords from the command line.'
				)}
			</Card.Root>

			<GithubRepoCard
				class="rounded-3xl shadow-none md:col-span-2 lg:col-span-2"
				url={repoUrl}
				description={siteDescription}
				facts={[
					{ label: 'Licence', value: 'MIT' },
					{ label: 'Version', value: version },
					{ label: 'Built with', value: 'Rust, Svelte' },
					{ label: 'Runs as', value: '1 container' }
				]}
			/>
		</div>
		<p class="mt-6"><a class="text-link" href={readmeUrl}>Setup guide on GitHub</a></p>
	</section>

	<section
		aria-labelledby="faq"
		class="grid gap-8 border-t pt-12 pb-20 sm:pb-22 md:grid-cols-[.9fr_1.1fr]"
	>
		<h2 id="faq" class="text-[clamp(34px,4vw,44px)]">Questions</h2>
		<Accordion.Root type="single">
			{#each faqs as f, i (i)}
				<Accordion.Item value="q{i}">
					<Accordion.Trigger level={3}>{f.q}</Accordion.Trigger>
					<Accordion.Content><p>{f.a}</p></Accordion.Content>
				</Accordion.Item>
			{/each}
		</Accordion.Root>
	</section>

	<!-- The sign-up ask as a ticket: the offer on the left, the price stamped on the stub. -->
	<Card.Root
		role="region"
		aria-labelledby="cta"
		class="ticket relative mb-20 grid grid-cols-[minmax(0,1fr)_var(--stub)] gap-0 rounded-3xl bg-primary py-0 text-primary-foreground [--stub:96px] sm:mb-22 sm:[--stub:200px]"
	>
		<div class="border-r-2 border-dashed border-primary-foreground/25 px-6 py-12 sm:px-12 sm:py-14">
			<h2 id="cta" class="max-w-[16ch] text-[clamp(30px,4.4vw,52px)]">
				Shorten your first link in under a minute.
			</h2>
			<div class="mt-8 flex flex-wrap gap-3">
				<Button
					href="/signup"
					class="bg-primary-foreground text-primary hover:bg-primary-foreground/90"
					>Create a free account</Button
				>
				<Button
					href="/login"
					variant="secondary"
					class="bg-primary-foreground/15 text-primary-foreground hover:bg-primary-foreground/25"
					>Sign in</Button
				>
			</div>
		</div>
		<div class="grid place-items-center p-2" aria-hidden="true">
			<Stamp
				label="Price"
				value="$0"
				rotate={-10}
				class="border-primary-foreground text-primary-foreground"
			/>
		</div>
	</Card.Root>
</div>

<style>
	/* The old address is struck through once, on load: the hero's one moment, then the stamp lands. */
	.strike {
		background: linear-gradient(var(--stamp), var(--stamp)) no-repeat 0 58% / 0 2px;
		animation: strike 700ms var(--ease-out) 200ms forwards;
	}
	@keyframes strike {
		to {
			background-size: 100% 2px;
		}
	}
</style>
