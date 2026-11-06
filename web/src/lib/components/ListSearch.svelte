<script lang="ts">
	import SearchIcon from '@lucide/svelte/icons/search';
	import * as InputGroup from '#lib/components/ui/input-group/index.js';
	import { Label } from '#lib/components/ui/label/index.js';

	let {
		id,
		label,
		placeholder,
		value = $bindable('')
	}: { id: string; label: string; placeholder: string; value?: string } = $props();
</script>

<div class="w-full sm:w-80">
	<Label class="sr-only" for={id}>{label}</Label>
	<InputGroup.Root>
		<InputGroup.Addon class="pr-2.5 pl-3.5"><SearchIcon /></InputGroup.Addon>
		<InputGroup.Input {id} type="search" {placeholder} bind:value />
	</InputGroup.Root>
</div>
