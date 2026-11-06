<script lang="ts">
	import type { ComponentProps } from 'svelte';
	import { onMount } from 'svelte';
	import * as InputGroup from '#lib/components/ui/input-group/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { shortHost } from '#lib/origin.ts';
	import { slugInput } from '#lib/slug-input.ts';

	let {
		value = $bindable(''),
		ref = $bindable(null),
		class: className,
		...rest
	}: Omit<ComponentProps<typeof InputGroup.Input>, 'type' | 'files'> = $props();

	// Prerendered pages carry origin.ts's stand-in host; hydrate on it, then show the real one.
	let mounted = $state(false);
	onMount(() => (mounted = true));
	const host = $derived(mounted ? shortHost : 'link.example');
</script>

<!-- Prefix and slug read as one address: muted host, bold slug, no gap between them. -->
<InputGroup.Root class={className}>
	<InputGroup.Addon class="max-w-[60%] pr-0 font-mono text-muted-foreground" title="{host}/">
		<InputGroup.Text class="block min-w-0 truncate text-[length:inherit]"
			><Skeleton loading={!mounted} class="inline-block">{host}/</Skeleton></InputGroup.Text
		>
	</InputGroup.Addon>
	<InputGroup.Input
		{...rest}
		bind:ref
		bind:value
		autocomplete="off"
		autocapitalize="off"
		spellcheck={false}
		class="pl-0 font-mono font-bold placeholder:font-normal placeholder:text-muted-foreground/60"
		{...slugInput((v) => (value = v))}
	/>
</InputGroup.Root>
