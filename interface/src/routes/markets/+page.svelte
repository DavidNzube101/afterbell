<script lang="ts">
	import { onMount } from 'svelte'
	import { goto } from '$app/navigation'
	import { siRobinhood } from 'simple-icons'
	import DashboardHeader from '../../lib/DashboardHeader.svelte'
	import TickerIcon from '../../lib/TickerIcon.svelte'
	import { markets as fallbackMarkets } from '../../lib/market-data'
	import { getMarketRegistry } from '../../lib/markets'
	import { initializeWalletSession, walletSession } from '../../lib/wallet-core'
	import { dollars, getMarketVolumes, getSnapshots, usdc, type MarketSnapshot } from '../../lib/dashboard'

	let markets = $state(fallbackMarkets as Array<{ ticker: string; name: string; tradingEnabled?: boolean }>)
	let snapshots = $state<MarketSnapshot[]>([])
	let volumes = $state<Record<string, string>>({})
	let snapshotsLoaded = $state(false)
	let volumesLoaded = $state(false)
	let query = $state('')
	let loadError = $state(false)
	let active = false
	let filteredMarkets = $derived(markets.filter(market => `${market.ticker} ${market.name} us equity robinhood chain`.toLowerCase().includes(query.trim().toLowerCase())))
	let visible = $state(true)
	let timer: number | undefined

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) { void goto('/', { replaceState: true }); return }
		active = true
		void getMarketRegistry().then(data => { if (active && data.markets.length) markets = data.markets }).catch(() => { loadError = true })
		void refresh()
		timer = window.setInterval(() => { if (document.visibilityState === 'visible') void refresh() }, 30_000)
		const onVisibility = () => { if (document.visibilityState === 'visible') void refresh() }
		document.addEventListener('visibilitychange', onVisibility)
		return () => { active = false; if (timer) window.clearInterval(timer); document.removeEventListener('visibilitychange', onVisibility) }
	})

	async function refresh() {
		if (document.visibilityState === 'hidden') return
		const [snapshotResult, volumeResult] = await Promise.allSettled([getSnapshots(), getMarketVolumes()])
		if (!active) return
		if (snapshotResult.status === 'fulfilled') { snapshots = snapshotResult.value; visible = true }
		else visible = false
		snapshotsLoaded = true
		if (volumeResult.status === 'fulfilled') volumes = Object.fromEntries(volumeResult.value.map(row => [row.market_id, row.volume_usdc_raw]))
		volumesLoaded = true
	}
	function money(value: number) { return `$${value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` }
</script>

<svelte:head><title>Markets | Afterbell</title><meta name="description" content="Explore supported equity perpetual markets on Afterbell." /></svelte:head>

{#if $walletSession}
<div class="trade-app markets-page">
	<DashboardHeader active="Markets" />
	<main class="markets-content">
		<div class="markets-title-row"><div><h1>Markets</h1><p>Explore the US equity markets available on Afterbell.</p>{#if loadError}<p class="markets-api-error">Market data is temporarily unavailable. Showing the configured market list.</p>{/if}</div></div>
		<section class="markets-directory" aria-label="Markets">
			<div class="markets-directory-toolbar"><label class="markets-search"><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></svg><input bind:value={query} placeholder="Search markets" aria-label="Search markets"/></label></div>
			<div class="markets-table-head"><span>Market</span><span>Type</span><span>Mark price</span><span>24h volume</span><span>Open interest</span><span>Funding / 8h</span><span>Network</span><span></span></div>
			<div class="markets-rows">
				{#each filteredMarkets as market (market.ticker)}
					{@const snapshot = snapshots.find(item => item.market_id === market.ticker)}
					<article class="supported-market-row">
						<div class="supported-market-name"><span class="market-brand-icon"><TickerIcon ticker={market.ticker} size={20}/></span><span><b>{market.ticker}</b><small>{market.name}</small></span></div>
						<span class="market-type">{market.ticker}-USDC</span>
						<span>{#if !snapshotsLoaded}<span class="market-skeleton" style="--skeleton-width:62px" aria-label="Loading market data"></span>{:else if snapshot?.mark_price_e8}{money(dollars(snapshot.mark_price_e8))}{:else}Unavailable{/if}</span>
						<span>{#if !volumesLoaded}<span class="market-skeleton" style="--skeleton-width:58px" aria-label="Loading market data"></span>{:else if volumes[market.ticker]}{money(usdc(volumes[market.ticker]))}{:else}$0.00{/if}</span>
						<span>{#if !snapshotsLoaded}<span class="market-skeleton" style="--skeleton-width:58px" aria-label="Loading market data"></span>{:else if snapshot}{money(usdc(snapshot.open_interest_usdc))}{:else}Unavailable{/if}</span>
						<span>{#if !snapshotsLoaded}<span class="market-skeleton" style="--skeleton-width:54px" aria-label="Loading market data"></span>{:else if snapshot}{(snapshot.funding_rate_bps_8h / 100).toFixed(4)}%{:else}Unavailable{/if}</span>
						<span class="market-network"><span class="robinhood-icon" role="img" aria-label="Robinhood Chain"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={siRobinhood.path}/></svg></span>Robinhood Chain</span>
						<div class="market-actions"><a class="market-open-link long" href={`/trade?market=${market.ticker}&side=long`}>Long</a><a class="market-open-link short" href={`/trade?market=${market.ticker}&side=short`}>Short</a></div>
					</article>
				{/each}
				{#if filteredMarkets.length === 0}<div class="markets-no-results">No supported markets match “{query}”.</div>{/if}
				{#if !visible}<p class="markets-api-error" role="status">Live market data is temporarily unavailable.</p>{/if}
			</div>
		</section>
	</main>
	<footer class="trade-statusbar"><span class="status-spacer"></span><a class="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
</div>
{/if}
