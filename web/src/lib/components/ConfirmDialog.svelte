<script lang="ts">
	import * as AlertDialog from '#lib/components/ui/alert-dialog/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import { Label } from '#lib/components/ui/label/index.js';
	import { pending } from '#lib/confirm.svelte.ts';

	const req = $derived(pending.req);
	let typed = $state('');
	const locked = $derived(!!req?.typeToConfirm && typed.trim() !== req.typeToConfirm);

	function settle(ok: boolean) {
		pending.req?.resolve(ok);
		pending.req = null;
		typed = '';
	}
</script>

<AlertDialog.Root open={!!req} onOpenChange={(o) => !o && settle(false)}>
	<AlertDialog.Content>
		{#if req}
			<AlertDialog.Header>
				<AlertDialog.Title>{req.title}</AlertDialog.Title>
				<AlertDialog.Description class="text-[15px]">{req.description}</AlertDialog.Description>
			</AlertDialog.Header>
			{#if req.typeToConfirm}
				<div class="grid gap-2">
					<Label for="confirm-text" class="block leading-normal"
						>Type <strong class="font-mono font-bold break-all">{req.typeToConfirm}</strong> to confirm</Label
					>
					<Input
						id="confirm-text"
						bind:value={typed}
						autocomplete="off"
						autocapitalize="off"
						spellcheck={false}
					/>
				</div>
			{/if}
			<AlertDialog.Footer>
				<AlertDialog.Cancel>{req.cancel ?? 'Cancel'}</AlertDialog.Cancel>
				<AlertDialog.Action
					variant={req.destructive ? 'destructive' : 'default'}
					disabled={locked}
					onclick={() => settle(true)}
				>
					{req.action}
				</AlertDialog.Action>
			</AlertDialog.Footer>
		{/if}
	</AlertDialog.Content>
</AlertDialog.Root>
