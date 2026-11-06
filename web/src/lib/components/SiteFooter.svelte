<script lang="ts">
	import { readmeUrl, repoUrl, version } from '#lib/site.ts';
	import { Separator } from '#lib/components/ui/separator/index.js';

	const links = [
		{ href: '/features', label: 'Features' },
		{ href: readmeUrl, label: 'Docs' },
		{ href: repoUrl, label: 'GitHub' },
		{ href: '/privacy', label: 'Privacy' },
		{ href: '/terms', label: 'Terms' },
		{ href: '/report', label: 'Report abuse' }
	];
</script>

<footer class="overflow-hidden">
	<Separator />
	<div class="wrap pt-12">
		<div class="flex flex-col gap-8 md:flex-row md:items-end md:justify-between">
			<p class="max-w-[22ch] font-serif text-[clamp(26px,3vw,34px)] leading-[1.15]">
				Short links you keep, on a server you run.
			</p>
			<nav aria-label="Footer">
				<ul class="flex flex-wrap gap-x-6 gap-y-2 text-[15px]">
					{#each links as l (l.label)}
						<li>
							<a
								class="text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
								href={l.href}>{l.label}</a
							>
						</li>
					{/each}
				</ul>
			</nav>
		</div>
		<!-- Oversized wordmark cropped by the page edge. -->
		<Separator class="mt-12" />
		<div class="flex items-end justify-between">
			<span
				aria-hidden="true"
				class="mb-[-0.26em] font-serif text-[clamp(120px,27vw,300px)] leading-none font-extrabold tracking-[-0.04em] text-rule select-none"
				>tiiny</span
			>
			<p class="mb-8 shrink-0 font-mono text-[13px] text-muted-foreground sm:mb-14">
				Version {version}
			</p>
		</div>
	</div>
</footer>
