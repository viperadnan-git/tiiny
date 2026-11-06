<!-- New password plus its repeat; the form's schema checks they match (see `confirmed` in schemas.ts). -->
<script lang="ts" generics="T extends Record<string, unknown> & { confirm: string }">
	import type { FormPath, SuperForm } from 'sveltekit-superforms';
	import * as Form from '#lib/components/ui/form/index.js';
	import PasswordInput from '#lib/components/PasswordInput.svelte';
	import { passwordMin } from '#lib/schemas.ts';

	let {
		form,
		name,
		label = 'Password'
	}: { form: SuperForm<T>; name: FormPath<T>; label?: string } = $props();
	// svelte-ignore state_referenced_locally
	const { form: data } = form;
	const confirm = 'confirm' as FormPath<T>;
</script>

<Form.Field {form} {name}>
	<Form.Control>
		{#snippet children({ props })}
			<Form.Label>{label}</Form.Label>
			<PasswordInput
				{...props}
				autocomplete="new-password"
				required
				bind:value={$data[name] as string}
			/>
		{/snippet}
	</Form.Control>
	<Form.Description>At least {passwordMin} characters.</Form.Description>
	<Form.FieldErrors />
</Form.Field>
<Form.Field {form} name={confirm}>
	<Form.Control>
		{#snippet children({ props })}
			<Form.Label>Confirm {label.toLowerCase()}</Form.Label>
			<PasswordInput {...props} autocomplete="new-password" required bind:value={$data.confirm} />
		{/snippet}
	</Form.Control>
	<Form.FieldErrors />
</Form.Field>
