import { shortHost } from './origin.ts';
import { slugFromInput } from './schemas.ts';

/** Spread on a slug `<Input>`: rewrites a pasted or committed short link on this site to its bare slug. */
export function slugInput(set: (slug: string) => void) {
	const bare = (v: string) => (v.includes('/') ? slugFromInput(v, shortHost) : null);
	const apply = (el: HTMLInputElement, slug: string) => {
		el.value = slug;
		el.setSelectionRange(slug.length, slug.length);
		set(slug);
		el.classList.remove('slug-rewrite');
		void el.offsetWidth;
		el.classList.add('slug-rewrite');
	};
	return {
		onpaste(e: ClipboardEvent) {
			const slug = bare(e.clipboardData?.getData('text/plain') ?? '');
			if (slug === null) return;
			e.preventDefault();
			apply(e.currentTarget as HTMLInputElement, slug);
		},
		onchange(e: Event) {
			const el = e.currentTarget as HTMLInputElement;
			const slug = bare(el.value);
			if (slug !== null) apply(el, slug);
		}
	};
}
