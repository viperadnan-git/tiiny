<!--
	Ported from Opensource UI "Copy to Clipboard" (components/buttons/copy-button.tsx),
	https://opensourceui.in/components/copy-button. MIT License, Copyright (c) 2026 Bidyut Kundu.
	Changes: Svelte 5, built on our Button, Paper tokens, faster crossfade, no "Copied" on a failed write.

	Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
	documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
	rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
	persons to whom the Software is furnished to do so, subject to the following conditions:

	The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
	Software.

	THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
	WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
	COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
	OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
-->
<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import { onDestroy } from 'svelte';
	import { Button, type ButtonSize, type ButtonVariant } from '#lib/components/ui/button/index.js';
	import { copyFailed, copyText } from '#lib/clipboard.ts';
	import { cn } from '#lib/utils.ts';

	let {
		value,
		copied = $bindable(false),
		resetMs = 1600,
		variant = 'secondary',
		size = 'sm',
		compact = false,
		ref = $bindable(null),
		class: className,
		...rest
	}: {
		value: string;
		copied?: boolean;
		resetMs?: number;
		variant?: ButtonVariant;
		size?: ButtonSize;
		/** Icon only below `sm`; pass an `aria-label`. */
		compact?: boolean;
		ref?: HTMLButtonElement | null;
		class?: string;
		'aria-label'?: string;
	} = $props();

	let timer: ReturnType<typeof setTimeout> | undefined;
	onDestroy(() => clearTimeout(timer));

	async function copy() {
		if (!(await copyText(value))) {
			copied = false;
			return copyFailed();
		}
		copied = true;
		clearTimeout(timer);
		if (resetMs) timer = setTimeout(() => (copied = false), resetMs);
	}

	// Outgoing layer fades fast; incoming waits for it, so the two never overlap.
	const layer = 'transition-opacity ease-out';
	const show = 'opacity-100 delay-100 duration-150';
	const hide = 'opacity-0 duration-100';
</script>

<Button
	bind:ref
	{variant}
	{size}
	class={cn(copied && 'text-stamp', compact && 'max-sm:w-9 max-sm:px-0', className)}
	onclick={copy}
	{...rest}
>
	<span class="relative size-4 shrink-0" aria-hidden="true">
		<span class="absolute inset-0 grid place-items-center {layer} {copied ? hide : show}"
			><CopyIcon class="size-4" /></span
		>
		<span class="absolute inset-0 grid place-items-center {layer} {copied ? show : hide}"
			><CheckIcon class="size-4" /></span
		>
	</span>
	<span class={cn('grid', compact && 'max-sm:hidden')}>
		<span class="col-start-1 row-start-1 {layer} {copied ? hide : show}" aria-hidden={copied}
			>Copy</span
		>
		<span class="col-start-1 row-start-1 {layer} {copied ? show : hide}" aria-hidden={!copied}
			>Copied</span
		>
	</span>
</Button>
<!-- Outside the button: its children are presentational, so a nested live region isn't announced reliably. -->
<span class="sr-only" aria-live="polite">{copied ? 'Copied' : ''}</span>
