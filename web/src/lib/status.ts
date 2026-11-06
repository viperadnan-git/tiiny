/** Copy for `StatusPage`. `status` null: the request never got an answer. */
export function statusCopy(status: number | null, reason = '') {
	const online = typeof navigator === 'undefined' || navigator.onLine;
	if (status === null || (status >= 500 && !online)) {
		return online
			? {
					code: 'offline',
					mark: 'Held',
					title: 'Couldn’t reach tiiny',
					text: 'The server didn’t answer. Check your connection, then try again.',
					retry: true
				}
			: {
					code: 'offline',
					mark: 'Held',
					title: 'You’re offline',
					text: 'This page needs a connection. Reconnect, then try again.',
					retry: true
				};
	}
	if (status === 404)
		return {
			code: status,
			mark: 'Unknown',
			title: 'This page doesn’t exist',
			text: 'The address may be mistyped, or the page has moved. Check it, or start over from the home page.',
			retry: false
		};
	if (status < 500)
		return {
			code: status,
			mark: 'Refused',
			title: 'You can’t open this page',
			text: reason || 'You may need to sign in with a different account.',
			retry: false
		};
	return {
		code: status,
		mark: 'Returned',
		title: 'Something broke on our side',
		text: 'The server couldn’t finish this request. Try again in a moment.',
		retry: true
	};
}
