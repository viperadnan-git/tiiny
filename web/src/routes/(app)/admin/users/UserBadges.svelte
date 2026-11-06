<script lang="ts">
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import type { User } from '#lib/types.ts';

	let { u, loading }: { u: User; loading: boolean } = $props();
</script>

{#if u.role === 'admin' || u.disabled || !u.email_verified}
	<div class="mt-2 flex flex-wrap gap-1.5">
		{#if u.role === 'admin'}<Skeleton {loading} class="rounded-[5px]"
				><Badge variant="stamp" class="h-5">admin</Badge></Skeleton
			>{/if}
		{#if u.disabled}<Skeleton {loading} class="rounded-[5px]"
				><Badge variant="tag-destructive">disabled</Badge></Skeleton
			>{/if}
		{#if !u.email_verified}<Skeleton {loading} class="rounded-[5px]"
				><Badge variant="tag">unverified</Badge></Skeleton
			>{/if}
	</div>
{/if}
