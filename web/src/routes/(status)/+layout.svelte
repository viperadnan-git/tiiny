<script lang="ts">
	import StatusFrame from '#lib/components/StatusFrame.svelte';

	let { children } = $props();
</script>

<StatusFrame>{@render children()}</StatusFrame>
