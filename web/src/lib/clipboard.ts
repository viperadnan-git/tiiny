import { toast } from 'svelte-sonner';

/** False when refused or when `navigator.clipboard` is missing (non-secure origins throw a TypeError). */
export async function copyText(text: string): Promise<boolean> {
	try {
		await navigator.clipboard.writeText(text);
		return true;
	} catch {
		return false;
	}
}

export const copyFailed = () =>
	toast.error("Couldn't copy", { description: 'Select the short link and copy it yourself.' });
