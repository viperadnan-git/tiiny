<script lang="ts" module>
	import { type VariantProps, tv } from 'tailwind-variants';

	export const badgeVariants = tv({
		base: 'h-5 gap-1 rounded-full border border-transparent px-2 py-0.5 text-xs font-medium transition-all has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&>svg]:size-3! group/badge inline-flex w-fit shrink-0 items-center justify-center overflow-hidden whitespace-nowrap focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring aria-invalid:border-destructive aria-invalid:focus-visible:ring-destructive/30 [&>svg]:pointer-events-none',
		variants: {
			variant: {
				seal: 'transition-none border-stamp text-stamp h-auto min-w-21 flex-col gap-0 rounded-lg border-[2.5px] bg-transparent px-2 py-1.5 text-center font-mono text-[11px] leading-[1.15] font-bold whitespace-nowrap uppercase sm:min-w-26 sm:px-2.5 sm:py-2 sm:text-[13px]',
				stamp:
					'border-stamp text-stamp h-auto rounded-[5px] border-[1.5px] bg-transparent px-1.5 py-0 font-mono text-xs font-normal',
				'stamp-destructive':
					'border-destructive text-destructive h-auto rounded-[5px] border-[1.5px] bg-transparent px-1.5 py-0 font-mono text-xs font-normal',
				default: 'bg-primary text-primary-foreground [a]:hover:bg-primary/80',
				secondary: 'bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80',
				destructive:
					'bg-destructive/10 [a]:hover:bg-destructive/20 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 text-destructive dark:bg-destructive/20',
				outline: 'border-border text-foreground [a]:hover:bg-muted',
				tag: 'border-border text-foreground rounded-[5px] font-mono font-normal',
				'tag-destructive':
					'bg-destructive/10 text-destructive dark:bg-destructive/20 rounded-[5px] font-mono font-normal',
				ghost: 'hover:bg-muted dark:hover:bg-muted/50',
				link: 'text-primary underline-offset-4 hover:underline'
			}
		},
		defaultVariants: {
			variant: 'default'
		}
	});

	export type BadgeVariant = VariantProps<typeof badgeVariants>['variant'];
</script>

<script lang="ts">
	import { cn, type WithElementRef } from '#lib/utils.js';
	import type { HTMLAnchorAttributes } from 'svelte/elements';

	let {
		ref = $bindable(null),
		href,
		class: className,
		variant = 'default',
		children,
		...restProps
	}: WithElementRef<HTMLAnchorAttributes> & {
		variant?: BadgeVariant;
	} = $props();
</script>

<svelte:element
	this={href ? 'a' : 'span'}
	bind:this={ref}
	data-slot="badge"
	{href}
	class={cn(badgeVariants({ variant }), className)}
	{...restProps}
>
	{@render children?.()}
</svelte:element>
