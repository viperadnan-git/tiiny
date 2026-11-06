<script lang="ts">
	import { page } from '$app/state';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { openReports } from '#lib/report-count.svelte.ts';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const sections = [
		{ href: '/admin/users', label: 'Users' },
		{ href: '/admin/links', label: 'Links' },
		{ href: '/admin/reports', label: 'Reports' },
		{ href: '/admin/retired', label: 'Retired slugs' }
	];
	const current = $derived(sections.find((s) => page.url.pathname.startsWith(s.href))?.href ?? '');

	// Phones scroll the tabs; an edge fades only while there's more to scroll that way.
	let list = $state<HTMLUListElement>();
	let fade = $state({ l: false, r: false });
	function update() {
		if (!list) return;
		fade = { l: list.scrollLeft > 1, r: list.scrollLeft + list.clientWidth < list.scrollWidth - 1 };
	}
	$effect(() => {
		void current;
		list
			?.querySelector('[aria-current=page]')
			?.scrollIntoView({ inline: 'center', block: 'nearest' });
		update();
	});
</script>

<div class="grid gap-8 sm:gap-10">
	<!-- Equal cells from sm, so the late report count can't move its neighbours. -->
	<nav aria-label="Admin sections" class="w-fit max-w-full rounded-full bg-muted">
		<ul
			bind:this={list}
			onscroll={update}
			style:--l={fade.l ? '28px' : '0px'}
			style:--r={fade.r ? '28px' : '0px'}
			class="flex scrollbar-none gap-1 overflow-x-auto mask-[linear-gradient(to_right,transparent,#000_var(--l),#000_calc(100%-var(--r)),transparent)] p-1 sm:grid sm:grid-cols-[repeat(4,1fr)] [&::-webkit-scrollbar]:hidden"
		>
			{#each sections as s (s.href)}
				<li class="shrink-0">
					<a
						href={s.href}
						aria-current={s.href === current ? 'page' : undefined}
						class="flex h-10 items-center justify-center rounded-full px-6 text-sm font-medium whitespace-nowrap text-muted-foreground transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none aria-[current=page]:bg-card aria-[current=page]:text-foreground aria-[current=page]:shadow-[0_1px_2px_rgba(26,36,33,.12)] aria-[current=page]:ring-1 aria-[current=page]:ring-border"
					>
						<!-- Badge hangs off the label, so the label stays centred and the late count shifts nothing. -->
						<span class="relative">
							{s.label}
							{#if s.href === '/admin/reports' && openReports.count}
								<Badge
									class="absolute top-1/2 left-full ml-1.5 h-4.5 min-w-4.5 -translate-y-1/2 bg-destructive px-1 text-background tabular-nums"
									>{openReports.count}<span class="sr-only"> open</span></Badge
								>
							{/if}
						</span>
					</a>
				</li>
			{/each}
		</ul>
	</nav>
	{@render children()}
</div>

<svelte:window onresize={update} />
