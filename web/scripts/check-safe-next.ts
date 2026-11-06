import { safeNext } from '../src/lib/safe-next.ts';
const o = 'http://localhost:8080';
const cases: [string | null, string | null][] = [
	['/dashboard', '/dashboard'],
	[
		'/dashboard?url=https%3A%2F%2Fa.b%2Fc%3Fd%3D1%23e',
		'/dashboard?url=https%3A%2F%2Fa.b%2Fc%3Fd%3D1%23e'
	],
	['/auth/verify?token=a%2Fb', '/auth/verify?token=a%2Fb'],
	['//evil.com', null],
	['/\\evil.com', null],
	['https://evil.com', null],
	['javascript:alert(1)', null],
	['/\t/evil.com', null],
	[' /x', null],
	[null, null],
	['', null]
];
for (const [i, want] of cases) {
	const got = safeNext(i, o);
	if (got !== want) {
		console.error('FAIL', JSON.stringify(i), got);
		process.exit(1);
	}
}
console.log(`safeNext: ${cases.length} cases ok`);
