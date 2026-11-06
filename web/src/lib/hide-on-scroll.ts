/** Sets `data-hidden` on scroll down, clears it on scroll up or near the top. Pinning rules live in `.auto-hide` CSS. */
export function hideOnScroll(node: HTMLElement) {
	let last = scrollY;
	const onScroll = () => {
		const y = scrollY;
		const top = y <= node.offsetHeight;
		if (!top && Math.abs(y - last) < 6) return;
		node.toggleAttribute('data-hidden', !top && y > last);
		last = y;
	};
	addEventListener('scroll', onScroll, { passive: true });
	return { destroy: () => removeEventListener('scroll', onScroll) };
}
