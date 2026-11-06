<script lang="ts">
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { sentence } from '#lib/api-client.ts';
	import { cn } from '#lib/utils.ts';
	import * as Alert from '#lib/components/ui/alert/index.js';

	let { message, class: className }: { message: string | null | undefined; class?: string } =
		$props();
</script>

{#if message}
	<Alert.Root
		variant="destructive"
		role="alert"
		class={cn('enter border-destructive/30 bg-destructive/5', className)}
	>
		<CircleAlertIcon aria-hidden="true" />
		<Alert.Description>{sentence(message)}</Alert.Description>
	</Alert.Root>
{/if}
