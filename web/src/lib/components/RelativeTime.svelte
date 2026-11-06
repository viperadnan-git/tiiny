<script lang="ts">
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';

	/** Unix seconds. */
	let { unix }: { unix: number } = $props();
	const date = $derived(new Date(unix * 1000));

	const units: [Intl.RelativeTimeFormatUnit, number][] = [
		['year', 31536000],
		['month', 2592000],
		['day', 86400],
		['hour', 3600],
		['minute', 60],
		['second', 1]
	];
	const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'always' });
	/** Largest whole unit, rounded: "3 minutes ago", "in 2 days". */
	const relative = $derived.by(() => {
		const diff = unix - Date.now() / 1000;
		const [unit, secs] = units.find(([, s]) => Math.abs(diff) >= s) ?? units[units.length - 1];
		return rtf.format(Math.round(diff / secs), unit);
	});
</script>

<Tooltip.Root>
	<Tooltip.Trigger>
		{#snippet child({ props })}
			<!-- svelte-ignore a11y_no_noninteractive_tabindex (focusable so keyboard users get the tooltip) -->
			<time {...props} datetime={date.toISOString()} tabindex={0}>{relative}</time>
		{/snippet}
	</Tooltip.Trigger>
	<Tooltip.Content
		>{date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}</Tooltip.Content
	>
</Tooltip.Root>
