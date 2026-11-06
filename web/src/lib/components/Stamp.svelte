<script lang="ts">
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { formatCompact, formatNumber } from '#lib/format.ts';
	import { cn } from '#lib/utils.ts';

	let {
		label,
		value,
		rotate = -6,
		land = false,
		delay = 0,
		class: className
	}: {
		label: string;
		value: string | number;
		rotate?: number;
		land?: boolean;
		delay?: number;
		class?: string;
	} = $props();

	const exact = $derived(typeof value === 'number' ? formatNumber(value) : value);
	// Compact counts (4.8K, 1.3M) stay within the badge's fixed minimum width.
	const shown = $derived(typeof value === 'number' ? formatCompact(value) : value);
</script>

<!-- Rubber stamp: never mix-blend-mode (Chrome mispaints it during stamp-in inside the card). -->
<Badge
	variant="seal"
	class={cn(land && 'stamp-in', className)}
	style="rotate: {rotate}deg;{land && delay ? ` animation-delay: ${delay}ms;` : ''}"
	title="{exact} {label.toLowerCase()}"
>
	<span aria-hidden="true">{label}</span>
	<b class="block text-[19px] leading-tight sm:text-[26px]" aria-hidden="true">{shown}</b>
	<span class="sr-only">{exact} {label.toLowerCase()}</span>
</Badge>
