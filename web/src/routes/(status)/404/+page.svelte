<script lang="ts">
	import StatusPage from '#lib/components/StatusPage.svelte';
</script>

<StatusPage
	code={404}
	mark="Missing"
	title="This short link doesn’t exist"
	text="Check the address for typos. Slugs are case-sensitive, so /Abc and /abc are different links."
	dest="nowhere yet"
/>
