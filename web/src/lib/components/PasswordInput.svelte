<script lang="ts">
	import EyeIcon from '@lucide/svelte/icons/eye';
	import EyeOffIcon from '@lucide/svelte/icons/eye-off';
	import type { ComponentProps } from 'svelte';
	import * as InputGroup from '#lib/components/ui/input-group/index.js';

	let {
		value = $bindable(''),
		...rest
	}: Omit<ComponentProps<typeof InputGroup.Input>, 'type' | 'files'> = $props();
	let shown = $state(false);
</script>

<InputGroup.Root>
	<InputGroup.Input
		{...rest}
		type={shown ? 'text' : 'password'}
		autocapitalize="off"
		spellcheck={false}
		bind:value
	/>
	<InputGroup.Addon align="inline-end" class="pr-1.5">
		<InputGroup.Button
			size="icon-sm"
			class="rounded-full"
			aria-label={shown ? 'Hide password' : 'Show password'}
			aria-pressed={shown}
			onclick={() => (shown = !shown)}
		>
			{#if shown}<EyeOffIcon />{:else}<EyeIcon />{/if}
		</InputGroup.Button>
	</InputGroup.Addon>
</InputGroup.Root>
