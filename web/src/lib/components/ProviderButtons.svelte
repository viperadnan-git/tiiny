<script lang="ts">
	import BrandIcon from '#lib/components/BrandIcon.svelte';
	import { Button } from '#lib/components/ui/button/index.js';
	import { Separator } from '#lib/components/ui/separator/index.js';
	import { enabledProviders, providerLabel } from '#lib/format.ts';
	import { withNext } from '#lib/safe-next.ts';
	import type { Providers } from '#lib/types.ts';

	let {
		providers,
		verb = 'Continue',
		next = null
	}: { providers: Providers; verb?: string; next?: string | null } = $props();
	const list = $derived(enabledProviders(providers));
</script>

{#if list.length}
	<div class="grid gap-2.5">
		{#each list as p (p)}
			<Button
				href={withNext(`/api/v1/auth/oauth/${p}/start`, next)}
				data-sveltekit-reload
				variant="secondary"
				class="w-full"
			>
				<BrandIcon name={p} />{verb} with {providerLabel[p]}
			</Button>
		{/each}
	</div>
	{#if providers.password}
		<div class="flex items-center gap-3 font-mono text-[13px] text-muted-foreground">
			<Separator class="flex-1" />or<Separator class="flex-1" />
		</div>
	{/if}
{/if}
