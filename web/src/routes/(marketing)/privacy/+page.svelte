<script lang="ts">
	import LegalPage from '../LegalPage.svelte';
	import Seo from '#lib/components/Seo.svelte';
</script>

<Seo
	title="Privacy · tiiny"
	description="What tiiny stores about accounts, guest links, abuse reports and clicks, and what it never stores: no visitor IP addresses, raw user agents or full referrer URLs."
	path="/privacy"
/>

<LegalPage title="What tiiny keeps, and what it doesn’t.">
	<p>
		This page describes the tiiny software. The people running this instance decide how long data is
		kept and who can sign up; ask them if you have questions about this server.
	</p>

	<h2>When someone clicks a short link</h2>
	<p>tiiny redirects them and adds one to a set of daily tallies for that link:</p>
	<ul>
		<li>the total for the day (UTC);</li>
		<li>the referring site’s host, such as <code>news.example.com</code>, or “direct”;</li>
		<li>the country, as a two-letter code;</li>
		<li>the device type, browser family and operating-system family.</li>
	</ul>
	<p>
		<strong>It never stores</strong> the visitor’s IP address, their raw user-agent string or the full
		referring URL. The IP address is used in memory to rate-limit requests and look up a country, then
		dropped. Clicking a short link sets no cookies. Known bots and link-preview crawlers are redirected
		but not counted.
	</p>

	<h2>Without an account</h2>
	<ul>
		<li>
			<strong>Guest links:</strong> if you shorten a link without an account, one cookie (<code
				>tiiny_guest</code
			>, or <code>__Host-tiiny_guest</code> on https) lets this browser list and later claim those
			links. Only its hash is stored. It lasts 400 days and is cleared when you sign in. On http
			browsers send it only with requests to <code>/api/v1</code>; on https they send it with every
			request to this site. Redirects never read or set it.
		</li>
		<li>
			<strong>Daily cap:</strong> to limit how many links one address can make per day, the creator’s
			IP address (for IPv6, its /64 network) is stored with a count for that day, and only that. Unverified
			accounts count against the same cap. These counts are purged after a day.
		</li>
		<li>
			<strong>Abuse reports:</strong> a report stores the short link, where it pointed, the reason, any
			details you wrote and your email if you gave one. Only admins see reports.
		</li>
	</ul>

	<h2>When you have an account</h2>
	<ul>
		<li>
			<strong>Profile:</strong> your email address, name, role, whether your email is verified, and when
			you joined.
		</li>
		<li><strong>Password:</strong> only a salted Argon2 hash, if you set one.</li>
		<li>
			<strong>Connected sign-ins:</strong> which provider (Google or GitHub) and that provider’s account
			ID.
		</li>
		<li>
			<strong>Sessions:</strong> one cookie, <code>tiiny_session</code>, that identifies a
			server-side session. It lasts 30 days and is removed when you sign out.
		</li>
		<li>
			<strong>Email links:</strong> verification and reset tokens are stored hashed and expire after 24
			hours and 1 hour.
		</li>
		<li>
			<strong>Links:</strong> each destination, slug, click total and when it was made or changed, plus
			a per-day count of links you created.
		</li>
	</ul>
	<p>Your theme choice is saved in your browser’s local storage, not on the server.</p>

	<h2>How long it’s kept</h2>
	<p>
		Daily click tallies are kept until the operator’s retention setting removes them; by default
		they stay. A link’s all-time total stays as long as the link. Deleting a link deletes its stats.
		Deleting your account deletes your links and their stats. The slugs of deleted permanent or
		blocked links are kept so they can’t point somewhere new, until an admin releases one.
	</p>

	<h2>Third parties</h2>
	<p>
		tiiny loads no third-party scripts, fonts or analytics. If you sign in with Google or GitHub,
		that provider sees the sign-in. If the operator configured email, messages go through their mail
		server.
	</p>
</LegalPage>
