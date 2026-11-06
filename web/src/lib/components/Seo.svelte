<script lang="ts">
	import { siteName } from '#lib/site.ts';
	import { baseUrl } from '#lib/origin.ts';

	let {
		title,
		description,
		path,
		jsonLd = []
	}: { title: string; description: string; path: string; jsonLd?: object[] } = $props();

	const url = $derived(`${baseUrl}${path}`);
	const image = `${baseUrl}/og.png`;
	const ld = (o: object) =>
		`<script type="application/ld+json">${JSON.stringify(o).replace(/</g, '\\u003c')}</` +
		'script>';
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<link rel="canonical" href={url} />
	<meta property="og:type" content="website" />
	<meta property="og:site_name" content={siteName} />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:url" content={url} />
	<meta property="og:image" content={image} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta
		property="og:image:alt"
		content="tiiny: an address label showing a short link, where it redirects to and a rubber stamp with its click count"
	/>
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content={image} />
	{#each jsonLd as o, i (i)}
		{@html ld(o)}
	{/each}
</svelte:head>
