<script lang="ts">
	import { shortHost } from '#lib/origin.ts';
	import { onMount, tick } from 'svelte';
	import { api } from '#lib/api-client.ts';
	import FormAlert from '#lib/components/FormAlert.svelte';
	import Seo from '#lib/components/Seo.svelte';
	import Stamp from '#lib/components/Stamp.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import * as Form from '#lib/components/ui/form/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import * as RadioGroup from '#lib/components/ui/radio-group/index.js';
	import { Textarea } from '#lib/components/ui/textarea/index.js';
	import { spaForm } from '#lib/forms.ts';
	import {
		emailMax,
		reportDetailsMax,
		reportFormSchema,
		reportReasons,
		slugFromInput,
		urlMax
	} from '#lib/schemas.ts';
	import SlugInput from '#lib/components/SlugInput.svelte';

	const reasons: Record<(typeof reportReasons)[number], string> = {
		phishing: 'Phishing or scam',
		malware: 'Malware',
		spam: 'Spam',
		illegal: 'Illegal content',
		other: 'Something else'
	};

	let sent = $state(false);
	let thanks = $state<HTMLElement | null>(null);
	let slugEl = $state<HTMLInputElement | null>(null);

	async function again() {
		sent = false;
		await tick();
		slugEl?.focus();
	}

	const form = spaForm(
		reportFormSchema,
		{ slug: '', reason: '' as never, details: '', contact_email: '' },
		async (body) => {
			await api('POST', '/reports', body);
			sent = true;
			await tick();
			thanks?.focus();
		}
	);
	const { form: formData, enhance, message, submitting } = form;

	// Prerendered: the query string only exists in the browser.
	onMount(() => {
		const q = new URLSearchParams(location.search).get('slug');
		if (q) $formData.slug = (slugFromInput(q, shortHost) ?? q).slice(0, urlMax);
	});
</script>

<Seo
	title="Report abuse · tiiny"
	description="Report a short link on this tiiny instance that leads to phishing, malware, spam or illegal content."
	path="/report"
/>

<div class="wrap pt-14 pb-20 md:pt-22 md:pb-24">
	<h1 class="max-w-[14ch] text-[clamp(40px,5.6vw,76px)] leading-[1.02] tracking-tight">
		Report a short link
	</h1>
	<p class="mt-5 max-w-[52ch] text-[17px] text-muted-foreground">
		If a link on this site leads somewhere harmful, tell us which one and why. An admin reviews
		every report and can disable the link.
	</p>

	<p role="status" class="sr-only">{sent ? 'Thanks. An admin will review it.' : ''}</p>
	<Card.Root class="mt-8 block max-w-170 gap-0 rounded-3xl py-0 shadow-paper">
		{#if sent}
			<div
				class="enter grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 px-6 py-8 sm:px-10 sm:py-10"
			>
				<div>
					<h2 bind:this={thanks} tabindex="-1" class="text-[clamp(26px,3.4vw,34px)] outline-none">
						Thanks, an admin will review it.
					</h2>
					<p class="mt-3 max-w-[44ch] text-muted-foreground">
						If you left an email, an admin may write to ask about it.
					</p>
					<Button variant="secondary" class="mt-6" onclick={again}>Report another link</Button>
				</div>
				<div class="w-21 pt-1 sm:w-26">
					<Stamp label="Report" value="Filed" rotate={-8} land />
				</div>
			</div>
		{:else}
			<form method="POST" use:enhance>
				<div class="grid gap-6 px-6 pt-7 pb-8 sm:px-10 sm:pt-9">
					<Form.Field {form} name="slug" class="gap-0">
						<Form.Control>
							{#snippet children({ props })}
								<Form.Label>Short link</Form.Label>
								<SlugInput
									{...props}
									bind:ref={slugEl}
									required
									placeholder="abc123"
									maxlength={urlMax}
									class="mt-2"
									bind:value={$formData.slug}
								/>
							{/snippet}
						</Form.Control>
						<Form.FieldErrors class="pt-2" />
					</Form.Field>

					<Form.Fieldset {form} name="reason" class="space-y-0">
						<Form.Legend>What's wrong with it?</Form.Legend>
						<RadioGroup.Root
							bind:value={$formData.reason}
							name="reason"
							class="mt-3 grid gap-2 sm:grid-cols-2"
						>
							{#each reportReasons as r (r)}
								<Form.Control>
									{#snippet children({ props })}
										<Form.Label
											class="flex min-h-11 cursor-pointer items-center gap-3 rounded-[10px] border bg-card px-3.5 py-2.5 font-normal transition-colors hover:border-foreground/30 has-data-[state=checked]:border-stamp has-data-[state=checked]:bg-secondary/60 data-fs-error:text-foreground"
										>
											<RadioGroup.Item value={r} {...props} />
											{reasons[r]}
										</Form.Label>
									{/snippet}
								</Form.Control>
							{/each}
						</RadioGroup.Root>
						<Form.FieldErrors class="pt-2" />
					</Form.Fieldset>

					<Form.Field {form} name="details" class="gap-0">
						<Form.Control>
							{#snippet children({ props })}
								<Form.Label
									>Details <span class="font-normal text-muted-foreground">(optional)</span
									></Form.Label
								>
								<Textarea
									{...props}
									aria-required="false"
									rows={4}
									maxlength={reportDetailsMax}
									placeholder="What does the page do? Where did you find the link?"
									class="mt-2 min-h-28"
									bind:value={$formData.details}
								/>
							{/snippet}
						</Form.Control>
						<Form.FieldErrors class="pt-2" />
					</Form.Field>

					<Form.Field {form} name="contact_email" class="gap-0">
						<Form.Control>
							{#snippet children({ props })}
								<Form.Label
									>Your email <span class="font-normal text-muted-foreground">(optional)</span
									></Form.Label
								>
								<Input
									{...props}
									type="email"
									autocomplete="email"
									maxlength={emailMax}
									class="mt-2"
									bind:value={$formData.contact_email}
								/>
							{/snippet}
						</Form.Control>
						<Form.Description class="pt-2"
							>Only the admins see it, and only to ask you about this report.</Form.Description
						>
						<Form.FieldErrors class="pt-2" />
					</Form.Field>
				</div>

				<Card.Footer
					class="grid gap-3 rounded-none border-t bg-secondary/60 px-6 py-5 sm:px-10 [.border-t]:pt-5"
				>
					<FormAlert message={$message} />
					<Form.Button
						size="lg"
						class="h-12 w-full text-base sm:w-auto sm:justify-self-end sm:px-8"
						disabled={$submitting}
					>
						{$submitting ? 'Sending…' : 'Send report'}
					</Form.Button>
				</Card.Footer>
			</form>
		{/if}
	</Card.Root>
</div>
