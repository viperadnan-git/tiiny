<script lang="ts">
	import BrandIcon from '#lib/components/BrandIcon.svelte';
	import { page } from '$app/state';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as NavigationMenu from '#lib/components/ui/navigation-menu/index.js';
	import Logo from '#lib/components/Logo.svelte';
	import { hideOnScroll } from '#lib/hide-on-scroll.ts';
	import { readmeUrl, repoUrl } from '#lib/site.ts';

	const links = [
		{ href: '/features', label: 'Features' },
		{ href: readmeUrl, label: 'Docs' }
	];
</script>

<header use:hideOnScroll class="auto-hide sticky top-0 z-40 pt-2 sm:pt-3">
	<div class="wrap">
		<div
			class="flex h-14 items-center gap-1 rounded-full border bg-[color-mix(in_oklab,var(--sheet)_82%,transparent)] pr-1.5 pl-4 shadow-paper backdrop-blur-md sm:gap-6 sm:pl-6"
		>
			<Logo class="mr-1 text-[24px] sm:mr-0" />
			<NavigationMenu.Root viewport={false} aria-label="Main" class="min-w-0 flex-none">
				<NavigationMenu.List class="gap-0">
					{#each links as l (l.label)}
						<NavigationMenu.Item>
							<NavigationMenu.Link
								href={l.href}
								active={page.url.pathname === l.href}
								aria-current={page.url.pathname === l.href ? 'page' : undefined}
								class="rounded-full px-2 py-1.5 text-sm font-medium sm:px-3 sm:text-[15px]"
								>{l.label}</NavigationMenu.Link
							>
						</NavigationMenu.Item>
					{/each}
					<NavigationMenu.Item>
						<NavigationMenu.Link
							href={repoUrl}
							aria-label="Source on GitHub"
							class="size-9 justify-center rounded-full p-0"
						>
							<BrandIcon name="github" />
						</NavigationMenu.Link>
					</NavigationMenu.Item>
				</NavigationMenu.List>
			</NavigationMenu.Root>
			<div class="ml-auto flex items-center gap-1.5">
				<Button href="/login" variant="ghost" class="hidden sm:inline-flex">Sign in</Button>
				<Button href="/signup" class="px-3.5 sm:px-5"
					><span class="sm:hidden">Sign up</span><span class="hidden sm:inline"
						>Start shortening</span
					></Button
				>
			</div>
		</div>
	</div>
</header>
