<script lang="ts">
	import { onMount } from 'svelte'
	import TickerIcon from './TickerIcon.svelte'
	import { showToast } from './toasts'
	type Position = { ticker: string; side: 'Long' | 'Short'; size: string; leverage: string; entry: string; mark: string; pnl: string; change: string }
	let { position, onClose }: { position: Position; onClose: () => void } = $props()
	const backgrounds = [
		{ id: 'gradient-1', label: 'Blue glow' }, { id: 'gradient-2', label: 'Violet glow' }, { id: 'gradient-3', label: 'Emerald glow' }, { id: 'gradient-4', label: 'Warm glow' },
		{ id: 'image-1', label: 'Image 1', image: '/assets/share-images/1.png' }, { id: 'image-2', label: 'Image 2', image: '/assets/share-images/2.png' }, { id: 'image-3', label: 'Image 3', image: '/assets/share-images/3.png' },
	]
	let background = $state('gradient-1')
	let customImage = $state<string | null>(null)
	let notice = $state('')
	let tickerIconContainer: HTMLSpanElement
	function chooseImage(event: Event) {
		const input = event.currentTarget as HTMLInputElement
		const file = input.files?.[0]
		if (!file) return
		const reader = new FileReader()
		reader.onload = () => { if (typeof reader.result === 'string') { customImage = reader.result; background = 'custom-image' } }
		reader.readAsDataURL(file)
		input.value = ''
	}
	function loadImage(source: string) { return new Promise<HTMLImageElement>((resolve, reject) => { const image = new Image(); image.onload = () => resolve(image); image.onerror = reject; image.src = source }) }
	async function download() {
		const canvas = document.createElement('canvas'); canvas.width = 1600; canvas.height = 900
		const ctx = canvas.getContext('2d')
		if (!ctx) { notice = 'Could not create trade card'; return }
		const colors = ['#102b48', '#452d71', '#155a45', '#72452c']
		const bg = backgrounds.find(item => item.id === background)
		const source = background === 'custom-image' ? customImage : bg?.image
		if (source) {
			try { const image = await loadImage(source); const scale = Math.max(canvas.width / image.width, canvas.height / image.height); const w = image.width * scale, h = image.height * scale; ctx.drawImage(image, (canvas.width - w) / 2, (canvas.height - h) / 2, w, h) }
			catch { ctx.fillStyle = '#111923'; ctx.fillRect(0, 0, canvas.width, canvas.height) }
			ctx.fillStyle = '#080d16a8'; ctx.fillRect(0, 0, canvas.width, canvas.height)
		} else {
			ctx.fillStyle = '#0c121b'; ctx.fillRect(0, 0, canvas.width, canvas.height)
			const glow = ctx.createRadialGradient(1400, 40, 10, 1400, 40, 900); glow.addColorStop(0, colors[Number(background.slice(-1)) - 1] ?? colors[0]); glow.addColorStop(1, '#0c121b'); ctx.fillStyle = glow; ctx.fillRect(0, 0, canvas.width, canvas.height)
		}
		try { const brand = await loadImage('/assets/afterbell-logo.png'); ctx.drawImage(brand, 86, 49, 54, 54) } catch { /* Retain the text wordmark. */ }
		ctx.fillStyle = position.side === 'Long' ? '#123b30' : '#3b2028'; ctx.beginPath(); ctx.roundRect(1370, 63, 140, 58, 18); ctx.fill()
		try { const tickerSvg = tickerIconContainer?.querySelector('svg'); if (tickerSvg) { const blob = new Blob([new XMLSerializer().serializeToString(tickerSvg)], { type: 'image/svg+xml' }); const url = URL.createObjectURL(blob); try { const icon = await loadImage(url); ctx.fillStyle = '#ffffffed'; ctx.beginPath(); ctx.roundRect(90, 304, 70, 70, 35); ctx.fill(); ctx.drawImage(icon, 108, 322, 34, 34) } finally { URL.revokeObjectURL(url) } } } catch { /* The ticker remains in the text label. */ }
		ctx.shadowColor = '#000b'; ctx.shadowBlur = 8; ctx.shadowOffsetY = 2; ctx.fillStyle = '#f1f5f9'; ctx.font = '600 38px system-ui, sans-serif'; ctx.fillText('Afterbell', 153, 91)
		ctx.fillStyle = position.side === 'Long' ? '#72e3b4' : '#ffabb3'; ctx.font = '600 25px system-ui, sans-serif'; ctx.fillText(position.side, 1400, 101)
		ctx.fillStyle = '#f4f7fb'; ctx.font = '650 76px system-ui, sans-serif'; ctx.fillText(`${position.ticker}-USDC`, 185, 365)
		ctx.font = '650 78px system-ui, sans-serif'; const pnlWidth = ctx.measureText(position.pnl).width; ctx.fillStyle = '#9de8c7'; ctx.font = '500 34px system-ui, sans-serif'; const changeWidth = ctx.measureText(position.change).width; const pnlX = 1510 - pnlWidth - changeWidth - 18; ctx.fillStyle = '#54dfa4'; ctx.font = '650 78px system-ui, sans-serif'; ctx.fillText(position.pnl, pnlX, 375); ctx.fillStyle = '#9de8c7'; ctx.font = '500 34px system-ui, sans-serif'; ctx.fillText(position.change, pnlX + pnlWidth + 18, 371)
		const details = [['Position size', position.size], ['Leverage', position.leverage], ['Entry price', position.entry], ['Mark price', position.mark]]
		details.forEach(([label, value], index) => { const x = index % 2 === 0 ? 90 : 800, y = index < 2 ? 545 : 695; ctx.fillStyle = '#a2afbe'; ctx.font = '500 25px system-ui, sans-serif'; ctx.fillText(label, x, y); ctx.fillStyle = '#f1f5f9'; ctx.font = '700 34px system-ui, sans-serif'; ctx.fillText(value, x, y + 48) })
		ctx.fillStyle = '#b5d7fb'; ctx.font = '500 23px system-ui, sans-serif'; ctx.fillText('afterbellfx.xyz', 1290, 835); ctx.shadowColor = 'transparent'; ctx.shadowBlur = 0; ctx.shadowOffsetY = 0
		const link = document.createElement('a'); link.download = `${position.ticker.toLowerCase()}-${position.side.toLowerCase()}-trade.png`; link.href = canvas.toDataURL('image/png'); link.click(); notice = 'Trade card downloaded'; showToast('success', notice)
	}
	onMount(() => { const key = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose() }; document.addEventListener('keydown', key); document.body.style.overflow = 'hidden'; return () => { document.removeEventListener('keydown', key); document.body.style.overflow = '' } })
</script>

<div class="risk-modal-backdrop" role="presentation" onmousedown={(event) => { if (event.target === event.currentTarget) onClose() }}>
	<div class="share-modal" role="dialog" aria-modal="true" aria-labelledby="share-title">
		<header class="risk-modal-header"><div><h2 id="share-title">Share your trade</h2><p>A snapshot of this position, ready to share.</p></div><button class="drawer-close" aria-label="Close share trade dialog" onclick={onClose}>×</button></header>
		<div class="share-modal-content">
			<article class={`share-trade-card ${background}`} style={background.startsWith('image-') || background === 'custom-image' ? `background-image:linear-gradient(90deg,#0b111ad9,#0b111a9c),url(${background === 'custom-image' ? customImage : backgrounds.find(item => item.id === background)?.image})` : undefined}>
				<div class="share-card-brand"><img src="/assets/afterbell-logo.png" alt=""/><span>Afterbell</span><span class={`share-card-side ${position.side.toLowerCase()}`}>{position.side}</span></div>
				<div class="share-card-main"><div class="share-card-market"><span class="stock-avatar"><span bind:this={tickerIconContainer}><TickerIcon ticker={position.ticker} size={20}/></span></span><div><b>{position.ticker}-USDC</b></div></div><div class="share-card-profit"><strong>{position.pnl}</strong><span>{position.change}</span></div></div>
				<div class="share-card-details"><div><span>Position size</span><b>{position.size}</b></div><div><span>Leverage</span><b>{position.leverage}</b></div><div><span>Entry price</span><b>{position.entry}</b></div><div><span>Mark price</span><b>{position.mark}</b></div></div><div class="share-card-footer">afterbellfx.xyz</div>
			</article>
			<div class="share-background-picker"><div class="share-background-heading">Background</div><div class="share-background-options">{#each backgrounds as item (item.id)}<button class={`share-background-option ${item.id}`} class:selected={background === item.id} aria-label={item.label} aria-pressed={background === item.id} title={item.label} style={item.image ? `background-image:linear-gradient(#080c1280,#080c1280),url(${item.image})` : undefined} onclick={() => background = item.id}></button>{/each}<label class="share-background-option custom-image" class:selected={background === 'custom-image'} title="Upload custom image" aria-label="Upload custom image" style={customImage ? `background-image:linear-gradient(#080c1280,#080c1280),url(${customImage})` : undefined}><input type="file" accept="image/*" onchange={chooseImage}/><span>+</span></label></div></div>
			{#if notice}<p class="share-notice" role="status">{notice}</p>{/if}
		</div>
		<footer class="share-modal-footer"><button class="share-action-button" onclick={() => void download()}>Download</button></footer>
	</div>
</div>
