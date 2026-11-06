import { errorMessage } from '#lib/api-client.ts';
import { searchQuerySchema } from '#lib/schemas.ts';

type Page<T> = { items: T[]; next_cursor: number | null };

export function paged<T>(
	fetchPage: (q: string | undefined, cursor: number | null) => Promise<Page<T>>,
	list?: { get: () => T[]; set: (v: T[]) => void }
) {
	let own = $state<T[]>([]);
	let search = $state('');
	let cursor = $state<number | null>(null);
	let loading = $state(true);
	let appending = $state(false);
	let loaded = $state(false);
	let error = $state<string | null>(null);
	let seq = 0;
	const get = list?.get ?? (() => own);
	const set = list?.set ?? ((v: T[]) => (own = v));

	async function load(append: boolean) {
		const mine = ++seq;
		loading = true;
		appending = append;
		error = null;
		try {
			const query = searchQuerySchema.safeParse(search);
			if (!query.success) {
				error = query.error.issues[0].message;
				return;
			}
			const page = await fetchPage(query.data, append ? cursor : null);
			if (mine !== seq) return;
			set(append ? [...get(), ...page.items] : page.items);
			cursor = page.next_cursor;
		} catch (e) {
			if (mine === seq) error = errorMessage(e);
		} finally {
			if (mine === seq) {
				loading = false;
				loaded = true;
			}
		}
	}

	$effect(() => {
		const q = search;
		const t = setTimeout(() => load(false), q ? 250 : 0);
		return () => clearTimeout(t);
	});

	return {
		get items() {
			return get();
		},
		set items(v: T[]) {
			set(v);
		},
		get search() {
			return search;
		},
		set search(v: string) {
			search = v;
		},
		get loading() {
			return loading;
		},
		get loaded() {
			return loaded;
		},
		/** A first load or a new search: rows give way to skeleton rows. */
		get skeleton() {
			return loading && !appending;
		},
		/** Load more: skeleton rows follow the loaded ones. */
		get appending() {
			return loading && appending;
		},
		get error() {
			return error;
		},
		/** Rows to render: `fill` (skeleton stand-ins) alone on a first load, after the items while loading more. */
		rows<F>(fill: F[]): (T | F)[] {
			if (loading && !appending) return fill;
			return loading && appending ? [...get(), ...fill] : get();
		},
		get cursor() {
			return cursor;
		},
		load,
		/** Reloads from scratch, showing the skeleton again (e.g. after a filter change). */
		reset() {
			set([]);
			loaded = false;
			void load(false);
		}
	};
}
