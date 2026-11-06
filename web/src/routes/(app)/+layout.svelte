<script lang="ts">
	import { page } from '$app/state';
	import AppHeader from './AppHeader.svelte';
	import SiteFooter from '#lib/components/SiteFooter.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();

	// Signed-out pages link to Terms and Privacy; the workspace stays footer-free.
	const publicPage = $derived(/^\/\(app\)\/(login|signup|auth)(\/|$)/.test(page.route.id ?? ''));
</script>

<svelte:head><meta name="robots" content="noindex" /></svelte:head>

<a
	href="#main"
	class="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:rounded-full focus:bg-card focus:px-4 focus:py-2"
	>Skip to content</a
>
<AppHeader me={data.me} providers={data.providers} />
<main id="main" class="wrap pt-10 pb-20 sm:pt-14">
	{@render children()}
</main>
{#if publicPage}<SiteFooter />{/if}
<ConfirmDialog />
