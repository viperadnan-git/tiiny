<script lang="ts">
	import { shortToday } from '#lib/format.ts';
	import { withNext } from '#lib/safe-next.ts';
	import { onMount } from 'svelte';
	import { goto, invalidate } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError, errorMessage } from '#lib/api-client.ts';
	import { verifySchema } from '#lib/schemas.ts';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import Envelope from '#lib/components/Envelope.svelte';
	import { shortHost } from '#lib/origin.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const today = shortToday();
	const titles = {
		working: 'Email verified',
		done: 'Email verified',
		already: 'Already verified',
		wrong: 'This link can’t be used',
		off: 'This link can’t be used',
		failed: 'Verify your email'
	};

	const token = page.url.searchParams.get('token') ?? '';
	// svelte-ignore state_referenced_locally
	let status = $state<'working' | 'done' | 'already' | 'wrong' | 'off' | 'failed'>(
		data.me?.email_verified ? 'already' : 'working'
	);
	let error = $state('');

	async function verify() {
		status = 'working';
		const body = verifySchema.safeParse({ token });
		if (!body.success) {
			status = 'wrong';
			return;
		}
		try {
			await api('POST', '/auth/verify-email', body.data);
			status = 'done';
			await invalidate('app:auth');
		} catch (e) {
			if (e instanceof ApiError && e.status === 401) {
				const next = `/auth/verify?token=${encodeURIComponent(token)}`;
				await goto(withNext('/login', next), { replaceState: true });
			} else if (e instanceof ApiError && e.status === 409) {
				status = 'already';
			} else if (e instanceof ApiError && e.status === 400) {
				status = 'wrong';
			} else if (e instanceof ApiError && e.status === 404) {
				status = 'off';
			} else {
				status = 'failed';
				error = errorMessage(e);
			}
		}
	}

	onMount(() => {
		if (status !== 'already') void verify();
	});
</script>

<svelte:head><title>Verify email · tiiny</title></svelte:head>

{#snippet letter(full: boolean)}
	<Envelope
		{full}
		from={[
			{ text: data.me?.name ?? '', placeholder: 'Your name', strong: true },
			{ text: data.me?.email ?? '', placeholder: 'you@example.com' }
		]}
		to={[{ text: 'Verified address', strong: true }, { text: shortHost }]}
		valid={status === 'done' || status === 'already' || status === 'wrong' || status === 'off'}
		mark={status === 'wrong' || status === 'off'
			? { label: 'Return to', value: 'sender' }
			: status === 'already'
				? { label: 'Already', value: 'verified' }
				: { label: 'Verified', value: today }}
		facts={[
			'A verified address lets you create links and reset your password by email.',
			'Verifying signs out your other devices.',
			'Only the newest verification email works.'
		]}
	/>
{/snippet}

<AuthShell title={titles[status]} label={letter} loading={status === 'working'}>
	{#if status === 'working' || status === 'done'}
		<!-- While checking, the confirmed state renders as its own skeleton. -->
		{@const loading = status === 'working'}
		{#if loading}<p class="sr-only" role="status">Checking your link…</p>{/if}
		<p role={loading ? undefined : 'status'}>
			<Skeleton {loading} class="inline-block"
				>You can create links now. Your other devices were signed out.</Skeleton
			>
		</p>
		<div class="justify-self-start">
			<Skeleton {loading} class="rounded-full"
				><Button href="/dashboard">Go to your links</Button></Skeleton
			>
		</div>
	{:else if status === 'already'}
		<p role="status">
			{#if data.me}<strong class="font-mono font-bold break-all">{data.me.email}</strong>{:else}This
				address{/if} is already verified. You don't need this link anymore.
		</p>
		<Button href="/dashboard" class="justify-self-start">Go to your links</Button>
	{:else if status === 'wrong'}
		<div class="grid gap-2" role="alert">
			<p>This link has expired, was already used, or belongs to a different account.</p>
			<p class="text-muted-foreground">
				Sign in to the right account, or send a new link from your dashboard.
			</p>
		</div>
		<Button href="/dashboard" class="justify-self-start">Go to your links</Button>
	{:else if status === 'off'}
		<div class="grid gap-2" role="alert">
			<p>This server no longer sends email, so verification links don’t work.</p>
			<p class="text-muted-foreground">Your account works without verifying.</p>
		</div>
		<Button href="/dashboard" class="justify-self-start">Go to your links</Button>
	{:else}
		<Alert.Root variant="destructive" role="alert"
			><Alert.Description>{error}</Alert.Description></Alert.Root
		>
		<Button onclick={verify} class="justify-self-start">Try again</Button>
	{/if}
</AuthShell>
