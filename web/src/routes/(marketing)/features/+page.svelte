<script lang="ts">
	import { readmeUrl } from '#lib/site.ts';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import Seo from '#lib/components/Seo.svelte';
	import { softwareLd } from '../content.ts';

	const sections = [
		{
			id: 'analytics',
			title: 'Counted, not tracked',
			lead: 'Every click adds to a tally. Nothing about the person is kept.',
			points: [
				'Clicks per day for the last 7, 30 or 90 days, or all time, in UTC.',
				'Top referring sites (host only), countries, devices, browsers and operating systems.',
				'Known bots and link-preview crawlers are redirected but not counted.',
				'No IP addresses, raw user-agent strings or full referrer URLs are stored.'
			]
		},
		{
			id: 'permanent',
			title: 'Temporary or permanent',
			lead: 'Pick how a link behaves once it’s out in the world.',
			points: [
				'Links start temporary and redirect with 302. Change the destination or the slug at any time, and every click counts.',
				'Permanent links redirect with 301, so browsers cache them for a day. They can’t be edited or made temporary again.',
				'Deleting a permanent link retires its slug forever; the address answers “gone” instead of pointing somewhere new.'
			]
		},
		{
			id: 'slugs',
			title: 'Names you choose',
			lead: 'Short links read like words, not noise.',
			points: [
				'Custom slugs of 3 to 64 letters, numbers, hyphens or underscores. Case-sensitive.',
				'Leave it blank for a random seven-character slug.',
				'Search your links by slug or destination; copy any link in one click.'
			]
		},
		{
			id: 'sign-in',
			title: 'Sign-in options',
			lead: 'The operator decides which doors are open.',
			points: [
				'Email and password, Google and GitHub; connect several to one account.',
				'With SMTP configured, tiiny sends email verification and password-reset links.',
				'Sign-ups can be closed, and admins can verify, disable or remove accounts.'
			]
		},
		{
			id: 'self-host',
			title: 'Self-hosting',
			lead: 'One binary, one container, your domain.',
			points: [
				'The dashboard is built into the binary; nothing else to deploy.',
				'Data lives in a local libSQL file, or in an embedded replica of a Turso primary.',
				'Daily link limits, blocked destination domains and stats retention are all settings.'
			]
		}
	];
</script>

<Seo
	title="Features · tiiny"
	description="Click analytics without tracking, temporary and permanent links, custom slugs, flexible sign-in and a single-binary self-hosted deployment."
	path="/features"
	jsonLd={[softwareLd]}
/>

<div class="wrap pt-14 pb-20 md:pt-22 md:pb-24">
	<h1 class="max-w-[16ch] text-[clamp(40px,5.6vw,76px)] leading-[1.02] tracking-tight">
		What a short link should do, and nothing it shouldn’t.
	</h1>
	<nav aria-label="On this page" class="-mx-4 mt-10 overflow-x-auto px-4 sm:mx-0 sm:px-0">
		<ul class="flex gap-2 whitespace-nowrap">
			{#each sections as s (s.id)}
				<li>
					<Button
						href="#{s.id}"
						variant="secondary"
						size="sm"
						class="font-medium text-muted-foreground hover:text-foreground">{s.title}</Button
					>
				</li>
			{/each}
		</ul>
	</nav>
	<div class="mt-8 grid gap-4 md:grid-cols-2">
		{#each sections as s, i (s.id)}
			<!-- The first section (privacy) spans the row: it is the reason tiiny exists. -->
			<Card.Root
				id={s.id}
				role="region"
				aria-labelledby="{s.id}-h"
				class="grid scroll-mt-24 content-start gap-6 rounded-3xl [--card-spacing:--spacing(6)] sm:[--card-spacing:--spacing(8)] {i ===
				0
					? 'md:col-span-2 md:grid-cols-[.8fr_1.2fr] md:gap-10'
					: ''}"
			>
				<Card.Header class="block">
					<Card.Title
						><h2 id="{s.id}-h" class="text-[28px] leading-[1.1] sm:text-[32px]">
							{s.title}
						</h2></Card.Title
					>
					<Card.Description class="mt-2">{s.lead}</Card.Description>
					{#if s.id === 'permanent'}<Badge variant="stamp" class="mt-4">permanent · 301</Badge>{/if}
				</Card.Header>
				<Card.Content class="block">
					<ul class="grid gap-3 text-[16px]">
						{#each s.points as p (p)}
							<li class="flex gap-3">
								<span class="mt-2.25 size-1.5 shrink-0 rounded-full bg-stamp" aria-hidden="true"
								></span>{p}
							</li>
						{/each}
					</ul>
				</Card.Content>
			</Card.Root>
		{/each}
		<Card.Root
			class="grid content-center justify-items-start gap-5 rounded-3xl border-primary bg-primary p-6 text-primary-foreground sm:p-8"
		>
			<p class="max-w-[18ch] font-serif text-[28px] leading-[1.15]">
				Run it on your own server in a few minutes.
			</p>
			<div class="flex flex-wrap gap-3">
				<Button
					href="/signup"
					class="bg-primary-foreground text-primary hover:bg-primary-foreground/90"
					>Create a free account</Button
				>
				<Button
					href={readmeUrl}
					variant="secondary"
					class="bg-primary-foreground/15 text-primary-foreground hover:bg-primary-foreground/25"
					>Read the setup guide</Button
				>
			</div>
		</Card.Root>
	</div>
</div>
