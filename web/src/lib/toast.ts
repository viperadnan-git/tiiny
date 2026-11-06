import { toast } from 'svelte-sonner';
import { errorMessage } from '#lib/api-client.ts';

/** Title says what failed ("Couldn't block /abc"); the description says why. */
export const toastError = (title: string, e: unknown) =>
	toast.error(title, { description: errorMessage(e) });
