<script lang="ts">
	import * as Avatar from '#lib/components/ui/avatar/index.js';
	import { avatarFor, initials } from '#lib/avatar.ts';

	let {
		email,
		name = null,
		class: className
	}: { email: string; name?: string | null; class?: string } = $props();
</script>

<Avatar.Root class={className}>
	{#await avatarFor(email) then src}{#if src}<Avatar.Image {src} alt="" />{/if}{/await}
	<Avatar.Fallback>{initials(name, email)}</Avatar.Fallback>
</Avatar.Root>
