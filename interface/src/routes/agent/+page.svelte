<script lang="ts">
	import { onMount } from 'svelte'
	import { goto } from '$app/navigation'
	import DashboardHeader from '../../lib/DashboardHeader.svelte'
	import TickerIcon from '../../lib/TickerIcon.svelte'
	import { markets } from '../../lib/market-data'
	import { initializeWalletSession, walletSession } from '../../lib/wallet-core'
	import { showToast } from '../../lib/toasts'

	let marketPair = $state('NVDA-USDC')
	let message = $state('')
	let textarea = $state<HTMLTextAreaElement>()
	let starterPage = $state(0)
	let carouselPaused = $state(false)
	let marketPickerOpen = $state(false)
	let carouselTimer: number | undefined
	const starters = [
		{ icon: '↗', label: 'Break down the latest price action' },
		{ icon: '◷', label: 'Explain funding and open interest' },
		{ icon: '⇄', label: 'Compare this market with another' },
		{ icon: '⌁', label: 'Help me understand my position' },
		{ icon: '⌖', label: 'What are the main risks in this market?' },
		{ icon: '＋', label: 'Explain margin and leverage' },
	]
	const starterPages = [starters.slice(0, 3), starters.slice(3, 6)]
	const pairs = markets.map(market => `${market.ticker}-USDC`)
	const selectedTicker = $derived(marketPair.replace('-USDC', ''))

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) void goto('/', { replaceState: true })
		if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			carouselTimer = window.setInterval(() => { if (!carouselPaused) starterPage = (starterPage + 1) % starterPages.length }, 4500)
		}
		const onKeydown = (event: KeyboardEvent) => { if (event.key === 'Escape') marketPickerOpen = false }
		document.addEventListener('keydown', onKeydown)
		return () => { if (carouselTimer) window.clearInterval(carouselTimer); document.removeEventListener('keydown', onKeydown) }
	})

	function moveCarousel(direction: number) { starterPage = (starterPage + direction + starterPages.length) % starterPages.length }

	function chooseStarter(prompt: string) {
		message = `${prompt} for ${marketPair.replace('-USDC', '')}.`
		requestAnimationFrame(() => { textarea?.focus(); resizeComposer() })
	}
	function resizeComposer() {
		if (!textarea) return
		textarea.style.height = 'auto'
		textarea.style.height = `${Math.min(textarea.scrollHeight, 190)}px`
	}
	function submit() {
		if (!message.trim()) return
		showToast('warning', 'Otto chat is coming soon. Your prompt has not been sent.')
	}
</script>

<svelte:head>
	<title>Otto | Afterbell</title>
	<meta name="description" content="Meet Otto, your Afterbell trading assistant." />
</svelte:head>

{#if $walletSession}
	<div class="trade-app otto-page">
		<DashboardHeader active="Otto" />
		<main class="otto-main">
			<section class="otto-home" aria-labelledby="otto-title">
				<div class="otto-identity"><img src="/assets/afterbell-otto-logo.png" alt="" /><span>Otto</span></div>
				<h1 id="otto-title">Your markets, in perspective.</h1>
				<p class="otto-subtitle">Ask Otto about markets, positions, and trading concepts.</p>

				<form class="otto-composer" onsubmit={(event) => { event.preventDefault(); submit() }}>
					<label class="otto-sr-only" for="otto-message">Message Otto</label>
					<textarea bind:this={textarea} bind:value={message} oninput={resizeComposer} onkeydown={(event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit() } }} id="otto-message" rows="2" maxlength="1000" placeholder="Ask Otto anything about the markets..."></textarea>
					<div class="otto-composer-bottom">
						<button type="button" class="otto-pair-picker" aria-label={`Choose market pair, currently ${marketPair}`} aria-haspopup="dialog" onclick={() => marketPickerOpen = true}>
							<TickerIcon ticker={selectedTicker} size={20}/><span>{marketPair}</span>
							<svg class="otto-chevron" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>
						</button>
						<div class="otto-composer-actions">
							<button type="submit" class="otto-send" aria-label="Send message" disabled={!message.trim()}>
								<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6"/></svg>
							</button>
						</div>
					</div>
				</form>

				<div class="otto-starter-carousel" role="region" aria-roledescription="carousel" aria-label="Starter prompts" onpointerenter={() => carouselPaused = true} onpointerleave={() => carouselPaused = false} onfocusin={() => carouselPaused = true} onfocusout={() => carouselPaused = false}>
					<div class="otto-starter-track" style={`transform:translateX(-${starterPage * 100}%)`}>
						{#each starterPages as page, pageIndex (pageIndex)}
							<div class="otto-starters" role="group" aria-label={`Starter prompts ${pageIndex + 1} of ${starterPages.length}`} aria-hidden={starterPage !== pageIndex} inert={starterPage !== pageIndex}>
								{#each page as starter (starter.label)}
									<button type="button" class="otto-starter" onclick={() => chooseStarter(starter.label)}><span aria-hidden="true">{starter.icon}</span>{starter.label}</button>
								{/each}
							</div>
						{/each}
					</div>
					<div class="otto-carousel-controls">
						<button type="button" aria-label="Previous starter prompts" onclick={() => moveCarousel(-1)}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m15 18-6-6 6-6"/></svg></button>
						<span>{#each starterPages as _, pageIndex (`dot-${pageIndex}`)}<button type="button" class:active={starterPage === pageIndex} aria-label={`Show starter prompts ${pageIndex + 1}`} aria-current={starterPage === pageIndex ? 'true' : undefined} onclick={() => starterPage = pageIndex}></button>{/each}</span>
						<button type="button" aria-label="Next starter prompts" onclick={() => moveCarousel(1)}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m9 18 6-6-6-6"/></svg></button>
					</div>
				</div>
				<p class="otto-disclaimer">Otto is an AI assistant. Review market information before making trading decisions.</p>
			</section>
		</main>
		<footer class="trade-statusbar otto-footer"><span class="status-spacer"></span><a class="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
	</div>
{/if}

{#if marketPickerOpen}
	<div class="otto-market-modal-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) marketPickerOpen = false }}>
		<dialog open class="otto-market-modal" aria-labelledby="otto-market-title">
			<header><div><h2 id="otto-market-title">Choose market pair</h2><p>Pick a market for Otto to focus on.</p></div><button type="button" class="otto-market-close" aria-label="Close market picker" onclick={() => marketPickerOpen = false}>×</button></header>
			<div class="otto-market-options">
				{#each markets as market (market.ticker)}
					<button type="button" class:selected={marketPair === `${market.ticker}-USDC`} onclick={() => { marketPair = `${market.ticker}-USDC`; marketPickerOpen = false }}>
						<TickerIcon ticker={market.ticker} size={30}/><span><b>{market.ticker}-USDC</b><small>{market.name}</small></span>{#if marketPair === `${market.ticker}-USDC`}<svg class="otto-market-check" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg>{/if}
					</button>
				{/each}
			</div>
		</dialog>
	</div>
{/if}
