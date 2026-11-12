<script lang="ts">
	import { onMount } from 'svelte'
	import { dollars, type PricePoint, type VolumePoint } from './dashboard'

	type Range = '1h' | '1d' | '1w' | '1m' | 'all'
	type Candle = { time: number; open: number; high: number; low: number; close: number; volume: number }
	let { ticker, name, points, volumePoints, range = '1h', onRangeChange }: {
		ticker: string
		name: string
		points: PricePoint[]
		volumePoints: VolumePoint[]
		range?: Range
		onRangeChange: (range: Range) => void
	} = $props()
	const intervals: Record<Range, number> = { '1h': 900_000, '1d': 3_600_000, '1w': 14_400_000, '1m': 86_400_000, all: 86_400_000 }
	let canvas = $state<HTMLCanvasElement>()
	let hoveredIndex = $state<number | null>(null)
	let candles = $derived.by(() => {
		const buckets = new Map<number, Candle>()
		const interval = intervals[range]
		const volumes = new Map<number, number>()
		for (const point of volumePoints) {
			const bucket = Math.floor(point.timestamp_ms / interval) * interval
			try {
				const amount = Number(BigInt(point.volume_usdg_raw)) / 1_000_000
				if (Number.isFinite(amount)) volumes.set(bucket, (volumes.get(bucket) ?? 0) + amount)
			} catch { /* Ignore invalid volume samples. */ }
		}
		for (const point of [...points].sort((a, b) => a.timestamp_ms - b.timestamp_ms)) {
			const value = dollars(point.price_e8)
			if (!Number.isFinite(value) || value <= 0) continue
			const bucket = Math.floor(point.timestamp_ms / interval) * interval
			const candle = buckets.get(bucket)
			if (candle) { candle.high = Math.max(candle.high, value); candle.low = Math.min(candle.low, value); candle.close = value }
			else buckets.set(bucket, { time: bucket, open: value, high: value, low: value, close: value, volume: 0 })
		}
		return [...buckets.values()].map(candle => ({ ...candle, volume: volumes.get(candle.time) ?? 0 })).sort((a, b) => a.time - b.time)
	})
	let displayCandle = $derived(hoveredIndex === null ? candles.at(-1) : candles[hoveredIndex] ?? candles.at(-1))

	function paint() {
		if (!canvas) return
		const ctx = canvas.getContext('2d')
		if (!ctx) return
		const width = canvas.clientWidth, height = canvas.clientHeight
		if (!width || !height) return
		const dpr = window.devicePixelRatio || 1
		const backingWidth = Math.round(width * dpr), backingHeight = Math.round(height * dpr)
		if (canvas.width !== backingWidth || canvas.height !== backingHeight) { canvas.width = backingWidth; canvas.height = backingHeight }
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
		ctx.clearRect(0, 0, width, height)
		if (!candles.length) return
		const left = 12, right = Math.min(width * .2, 76), top = 20, bottom = 26
		const volumeHeight = Math.min(74, Math.max(42, height * .18)), plotBottom = height - bottom - volumeHeight - 12
		const plotWidth = width - left - right, plotHeight = plotBottom - top
		if (plotWidth <= 0 || plotHeight <= 0) return
		const minPrice = Math.min(...candles.map(c => c.low)), maxPrice = Math.max(...candles.map(c => c.high))
		const padding = Math.max((maxPrice - minPrice) * .1, maxPrice * .001)
		const low = Math.max(0, minPrice - padding), high = maxPrice + padding
		const start = candles[0].time - intervals[range] / 2, end = candles.at(-1)!.time + intervals[range] / 2
		const span = Math.max(intervals[range], end - start)
		const x = (time: number) => left + (time - start) / span * plotWidth
		const y = (price: number) => top + (high - price) / (high - low || 1) * plotHeight
		ctx.font = `12px ${getComputedStyle(canvas).fontFamily}`
		ctx.fillStyle = '#18212d'
		for (let gx = left; gx <= width - right; gx += 24) for (let gy = top; gy <= height - bottom; gy += 24) { ctx.beginPath(); ctx.arc(gx, gy, .7, 0, Math.PI * 2); ctx.fill() }
		ctx.fillStyle = '#8492a3'; ctx.textAlign = 'left'
		for (let tick = 0; tick <= 4; tick++) {
			const value = high - (high - low) * tick / 4
			ctx.fillText(value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 }), width - right + 8, top + plotHeight * tick / 4 + 4)
		}
		const maxVolume = Math.max(1, ...candles.map(c => c.volume))
		const bodyWidth = Math.max(1, Math.min(12, plotWidth * intervals[range] / span * .72))
		for (const candle of candles) {
			const cx = x(candle.time), color = candle.close >= candle.open ? '#18c77a' : '#ff583f'
			ctx.strokeStyle = color; ctx.lineWidth = 1; ctx.beginPath(); ctx.moveTo(cx, y(candle.high)); ctx.lineTo(cx, y(candle.low)); ctx.stroke()
			ctx.fillStyle = color; ctx.fillRect(cx - bodyWidth / 2, Math.min(y(candle.open), y(candle.close)), bodyWidth, Math.max(1.5, Math.abs(y(candle.close) - y(candle.open))))
			if (candle.volume > 0) { const bar = candle.volume / maxVolume * (volumeHeight - 4); ctx.globalAlpha = .55; ctx.fillRect(cx - bodyWidth / 2, height - bottom - bar, bodyWidth, bar); ctx.globalAlpha = 1 }
		}
		const last = candles.at(-1)!, lastY = y(last.close), up = last.close >= last.open
		ctx.strokeStyle = 'rgba(132, 146, 163, .42)'; ctx.setLineDash([4, 5]); ctx.beginPath(); ctx.moveTo(left, lastY); ctx.lineTo(width - right, lastY); ctx.stroke(); ctx.setLineDash([])
		ctx.fillStyle = '#10161f'; ctx.fillRect(width - right + 2, lastY - 12, right - 4, 24); ctx.strokeStyle = up ? '#18c77a' : '#ff583f'; ctx.strokeRect(width - right + 2, lastY - 12, right - 4, 24); ctx.fillStyle = up ? '#18c77a' : '#ff583f'; ctx.fillText(last.close.toFixed(2), width - right + 8, lastY + 4)
		ctx.fillStyle = '#8492a3'; const tickCount = width < 480 ? 3 : 5
		for (let tick = 0; tick < tickCount; tick++) {
			const time = start + span * tick / (tickCount - 1)
			const options: Intl.DateTimeFormatOptions = ['1w', '1m', 'all'].includes(range) ? { month: 'short', day: 'numeric' } : { hour: '2-digit', minute: '2-digit' }
			ctx.textAlign = tick === 0 ? 'left' : tick === tickCount - 1 ? 'right' : 'center'
			ctx.fillText(new Date(time).toLocaleString([], options), x(time), height - 6)
		}
		ctx.textAlign = 'left'
		if (hoveredIndex !== null) {
			const candle = candles[hoveredIndex]
			if (candle) { const cx = x(candle.time); ctx.strokeStyle = 'rgba(132,146,163,.55)'; ctx.lineWidth = .7; ctx.beginPath(); ctx.moveTo(cx, top); ctx.lineTo(cx, height - bottom); ctx.stroke() }
		}
	}
	function onPointerMove(event: PointerEvent) {
		const element = canvas
		if (!candles.length || !element) return
		const rect = element.getBoundingClientRect(), width = element.clientWidth
		const start = candles[0].time - intervals[range] / 2, end = candles.at(-1)!.time + intervals[range] / 2
		const right = Math.min(width * .2, 76), t = start + (event.clientX - rect.left - 12) / (width - 12 - right) * (end - start)
		let nearest = 0
		for (let i = 1; i < candles.length; i++) if (Math.abs(candles[i].time - t) < Math.abs(candles[nearest].time - t)) nearest = i
		if (hoveredIndex !== nearest) hoveredIndex = nearest
	}
	onMount(() => {
		const resize = new ResizeObserver(paint)
		if (canvas?.parentElement) resize.observe(canvas.parentElement)
		return () => resize.disconnect()
	})
	$effect(() => { candles; range; hoveredIndex; requestAnimationFrame(paint) })
</script>

<div class="chart-shell">
	<div class="chart-tools"><div class="chart-pair"><span>{ticker} / USDC</span></div><div class="chart-actions" role="group" aria-label="Chart time range">{#each ['1h', '1d', '1w', '1m', 'all'] as item (item)}<button class="chart-range-button" class:active={range === item} onclick={() => onRangeChange(item as Range)}>{item.toUpperCase()}</button>{/each}</div></div>
	<div class="real-market-chart" role="img" aria-label={`${name} candlestick chart`}>
		{#if candles.length === 0}<div class="chart-placeholder"><span>Waiting for fresh market price data.</span></div>
		{:else}<canvas bind:this={canvas} class="oracle-candlestick-canvas" aria-label={`${ticker} / USDC OHLC price history`} onpointermove={onPointerMove} onpointerleave={() => hoveredIndex = null}></canvas>
			{#if displayCandle}<div class="chart-ohlc" aria-label="Selected candle"><span>O <b>{displayCandle.open.toFixed(2)}</b></span><span>H <b>{displayCandle.high.toFixed(2)}</b></span><span>L <b>{displayCandle.low.toFixed(2)}</b></span><span>C <b>{displayCandle.close.toFixed(2)}</b></span></div>{/if}
		{/if}
	</div>
</div>
