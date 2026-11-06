<script lang="ts">
	import { cn } from '#lib/utils.ts';

	let { href = '/', class: className }: { href?: string; class?: string } = $props();
</script>

<!-- min-w: the Playfair 800 width; the Georgia fallback is narrower, so the nav would move when the font swaps. -->
<a
	{href}
	class={cn(
		'inline-block min-w-[2.03em] font-serif text-[26px] leading-none font-extrabold tracking-[-0.02em]',
		className
	)}>tiiny</a
>
