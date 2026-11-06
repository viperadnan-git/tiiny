<script lang="ts">
	import { BarChart } from 'layerchart';
	import * as Chart from '#lib/components/ui/chart/index.js';
	import { Skeleton } from '#lib/components/ui/skeleton/index.js';
	import { formatNumber } from '#lib/format.ts';
	import type { DayCount } from '#lib/types.ts';

	/** `null` while loading: same caption and chart box, shimmering. */
	let { series }: { series: DayCount[] | null } = $props();

	const dayFmt = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeZone: 'UTC' });
	const tickFmt = new Intl.DateTimeFormat(undefined, {
		month: 'short',
		day: 'numeric',
		timeZone: 'UTC'
	});
	const fmtDay = (d: Date | number) => dayFmt.format(d);

	const rows = $derived(
		(series ?? []).map((d) => ({ date: new Date(d.day * 86_400_000), count: d.count }))
	);
	const total = $derived(rows.reduce((n, d) => n + d.count, 0));
	const peak = $derived(rows.reduce((p, d) => (d.count > p.count ? d : p), rows[0]));
	const summary = $derived(
		rows.length
			? `Clicks per day from ${fmtDay(rows[0].date)} to ${fmtDay(rows[rows.length - 1].date)}: ${formatNumber(total)} in total, peak ${formatNumber(peak.count)} on ${fmtDay(peak.date)}.`
			: 'No clicks in this range.'
	);
	// Band scale: one label centred under its own bar. Wider ranges label every `step`th day, offset so neither edge label overhangs the plot.
	const step = $derived(Math.ceil(rows.length / 7));
	const xTicks = $derived(
		rows.filter((_, i) => step === 1 || i % step === Math.floor(step / 2)).map((d) => d.date)
	);
	const config = { count: { label: 'Clicks', color: 'var(--stamp)' } } satisfies Chart.ChartConfig;
</script>

<figure class="grid gap-4">
	<figcaption class="text-sm font-semibold">
		Clicks per day <span class="font-normal text-muted-foreground">(UTC)</span>
	</figcaption>
	{#if !series}
		<Skeleton class="h-60 w-full rounded-xl" />
	{:else}
		<div role="img" aria-label={summary}>
			<Chart.Container {config} class="aspect-auto h-60 w-full">
				<BarChart
					data={rows}
					x="date"
					y="count"
					bandPadding={0.25}
					series={[{ key: 'count', label: 'Clicks', color: config.count.color }]}
					props={{
						bars: { rounded: 'top', radius: 2, strokeWidth: 0 },
						xAxis: { format: (d: Date) => tickFmt.format(d), ticks: xTicks },
						// Counts are whole; few ticks when the peak is small, so the axis never shows 0.2 clicks.
						yAxis: {
							format: (v: number) => (Number.isInteger(v) ? formatNumber(v) : ''),
							ticks: Math.max(1, Math.min(5, peak?.count ?? 1))
						},
						grid: { y: true }
					}}
				>
					{#snippet tooltip()}
						<Chart.Tooltip labelFormatter={(d: Date) => fmtDay(d)} />
					{/snippet}
				</BarChart>
			</Chart.Container>
		</div>
	{/if}
</figure>
