<script lang="ts">
	import * as FormPrimitive from 'formsnap';
	import CircleAlertIcon from '@lucide/svelte/icons/circle-alert';
	import { sentence } from '#lib/api-client.ts';
	import { cn, type WithoutChild } from '#lib/utils.js';

	let {
		ref = $bindable(null),
		class: className,
		errorClasses,
		children: childrenProp,
		...restProps
	}: WithoutChild<FormPrimitive.FieldErrorsProps> & {
		errorClasses?: string | undefined | null;
	} = $props();
</script>

<FormPrimitive.FieldErrors
	bind:ref
	class={cn('text-sm text-destructive', className)}
	{...restProps}
>
	{#snippet children({ errors, errorProps })}
		{#if childrenProp}
			{@render childrenProp({ errors, errorProps })}
		{:else}
			{#if errors[0]}
				<div {...errorProps} class={cn('enter flex items-start gap-1.5', errorClasses)}>
					<CircleAlertIcon class="mt-0.75 size-3.5 shrink-0" aria-hidden="true" />{sentence(
						errors[0]
					)}
				</div>
			{/if}
		{/if}
	{/snippet}
</FormPrimitive.FieldErrors>
