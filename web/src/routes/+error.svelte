<script lang="ts">
	import { page } from '$app/state';
	import { Button } from '#lib/components/ui/button/index.js';
	import StatusFrame from '#lib/components/StatusFrame.svelte';
	import StatusPage from '#lib/components/StatusPage.svelte';
	import { statusCopy } from '#lib/status.ts';

	const status = $derived(page.status);
	// SvelteKit's stock messages say nothing the title doesn't; a server's own reason does.
	const reason = $derived(
		['Not Found', 'Internal Error'].includes(page.error?.message ?? '')
			? ''
			: (page.error?.message ?? '')
	);
	const copy = $derived(statusCopy(page.status, reason));
</script>

<StatusFrame>
	<StatusPage
		code={status}
		mark={copy.mark}
		title={copy.title}
		text={copy.text}
		label="Returned from"
		dest={page.url.pathname}
	>
		{#snippet actions()}
			{#if copy.retry}
				<Button onclick={() => location.reload()}>Try again</Button>
				<a
					class="text-[15px] text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
					href="/">Go to tiiny</a
				>
			{:else}
				<Button href="/">Go to tiiny</Button>
			{/if}
		{/snippet}
	</StatusPage>
</StatusFrame>
