<script lang="ts">
	import './layout.css';
	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from '#lib/components/ui/sonner/index.js';
	import * as Tooltip from '#lib/components/ui/tooltip/index.js';

	let { children } = $props();
</script>

<ModeWatcher />
<Toaster position="bottom-right" />
<Tooltip.Provider delayDuration={300}>
	{@render children()}
</Tooltip.Provider>
