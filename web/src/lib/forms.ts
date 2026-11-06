import {
	defaults,
	setError,
	superForm,
	type FormOptions,
	type SuperValidated
} from 'sveltekit-superforms';
import { zod as zod4, zodClient as zod4Client } from 'sveltekit-superforms/adapters/zod4';
import type { z } from 'zod';
import { ApiError, errorMessage } from './api-client.ts';

type Schema = z.ZodType<Record<string, unknown>, Record<string, unknown>>;

/** Server messages name their field first ("slug is already taken"); anything else is a form-level message. */
function serverError(form: SuperValidated<Record<string, unknown>, string>, e: unknown) {
	const msg = errorMessage(e);
	// Match the server's raw lowercase message: `msg` is already capitalized.
	const raw = e instanceof ApiError ? e.message : '';
	const field = Object.keys(form.data).find((k) => raw.startsWith(`${k.replaceAll('_', ' ')} `));
	if (field) setError(form, field as never, msg);
	else form.message = msg;
}

let formCount = 0;

/** Client-only superform: the zod schema validates, `submit` calls the API with the parsed data. */
export function spaForm<S extends Schema>(
	schema: S,
	initial: z.output<S>,
	submit: (data: z.output<S>) => Promise<void>,
	options: FormOptions<z.output<S>, string, z.input<S>> = {}
) {
	const form = superForm(
		defaults(initial, zod4(schema as never)) as SuperValidated<z.output<S>, string, z.input<S>>,
		{
			SPA: true,
			// Superforms ids by schema, so two live forms of one schema (two edit dialogs mid-navigation) collide; SPA forms never share action data.
			id: `spa-${++formCount}`,
			// No SvelteKit action to apply; applyAction would also reset focus after a submit that navigated.
			applyAction: false,
			validators: zod4Client(schema as never),
			...options,
			async onUpdate({ form }) {
				if (!form.valid) return;
				try {
					await submit(form.data);
				} catch (e) {
					form.valid = false;
					serverError(form as never, e);
				}
			}
		}
	);
	// Zod is the only validator, so its messages show inline instead of the browser's bubbles.
	const enhance = form.enhance;
	form.enhance = (el, events) => {
		el.noValidate = true;
		return enhance(el, events);
	};
	return form;
}
