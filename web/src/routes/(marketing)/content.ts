import { repoUrl, version, siteDescription } from '#lib/site.ts';
import { baseUrl } from '#lib/origin.ts';

export const faqs = [
	{
		q: 'What does tiiny record when someone clicks?',
		a: 'The day, the referring site’s host, the country, and the device, browser and operating-system family. It never stores IP addresses, raw user-agent strings or full referrer URLs.'
	},
	{
		q: 'What’s the difference between temporary and permanent links?',
		a: 'Temporary links redirect with 302: you can change the destination or the slug, and every click is counted. Permanent links redirect with 301, so browsers may cache them. They can’t be edited, repeat visits from the same browser aren’t counted, and deleting one retires its slug for good.'
	},
	{
		q: 'Can I choose the short name?',
		a: 'Yes. Use 3 to 64 letters, numbers, hyphens or underscores. Slugs are case-sensitive, so /Docs and /docs are different links. Leave it blank and tiiny picks a random seven-character slug.'
	},
	{
		q: 'Do I need an email server?',
		a: 'No. Without SMTP, accounts work straight away and the operator resets passwords from the command line. With SMTP, tiiny sends verification and password-reset links.'
	},
	{
		q: 'Can I run more than one instance?',
		a: 'No. tiiny is built for a single instance: its redirect cache, rate limiter and click buffer live in memory. One container handles a lot of redirects.'
	}
];

export const softwareLd = {
	'@context': 'https://schema.org',
	'@type': 'SoftwareApplication',
	name: 'tiiny',
	description: siteDescription,
	applicationCategory: 'UtilitiesApplication',
	operatingSystem: 'Linux (Docker)',
	softwareVersion: version,
	url: baseUrl,
	codeRepository: repoUrl,
	license: 'https://opensource.org/licenses/MIT',
	author: { '@type': 'Person', name: 'Adnan Ahmad' },
	offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }
};

export const faqLd = {
	'@context': 'https://schema.org',
	'@type': 'FAQPage',
	mainEntity: faqs.map((f) => ({
		'@type': 'Question',
		name: f.q,
		acceptedAnswer: { '@type': 'Answer', text: f.a }
	}))
};
