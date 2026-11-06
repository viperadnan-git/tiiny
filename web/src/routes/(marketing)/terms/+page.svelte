<script lang="ts">
	import LegalPage from '../LegalPage.svelte';
	import * as Alert from '#lib/components/ui/alert/index.js';
	import Seo from '#lib/components/Seo.svelte';
</script>

<Seo title="Terms · tiiny" description="Terms of use for this tiiny instance." path="/terms" />

<LegalPage title="Terms of use">
	{#snippet notice()}
		<Alert.Root class="border-stamp">
			<Alert.Title class="font-semibold">Placeholder for the operator</Alert.Title>
			<Alert.Description>
				These are generic terms shipped with tiiny. If you run this instance, replace them with your
				own in <code class="font-mono break-all">web/src/routes/(marketing)/terms/+page.svelte</code
				> before inviting people.
			</Alert.Description>
		</Alert.Root>
	{/snippet}
	<h2>Using this service</h2>
	<p>
		You can create short links to content you’re allowed to share. Don’t use this service for spam,
		phishing, malware, or anything illegal where you or the operator are.
	</p>
	<h2>Your links</h2>
	<p>
		You’re responsible for where your links point. The operator may disable links or accounts that
		break these terms, and may block destination domains.
	</p>
	<h2>Availability</h2>
	<p>
		This service is provided as is, without any warranty. The operator may change or stop it at any
		time. Permanent links are cached by browsers and can’t be recalled once shared.
	</p>
	<h2>Privacy</h2>
	<p>See the <a href="/privacy">privacy page</a> for what is stored.</p>
	<h2>Changes</h2>
	<p>
		The operator may update these terms. Continuing to use the service means you accept the current
		version.
	</p>
</LegalPage>
