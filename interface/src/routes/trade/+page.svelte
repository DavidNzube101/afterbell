<script lang="ts">
	import { onMount } from 'svelte'
	import { goto } from '$app/navigation'
	import OrderPanel from '../../lib/OrderPanel.svelte'
	import DashboardHeader from '../../lib/DashboardHeader.svelte'
	import AccountBalanceModal from '../../lib/AccountBalanceModal.svelte'
	import ShareTradeModal from '../../lib/ShareTradeModal.svelte'
	import RiskDisclosureModal from '../../lib/RiskDisclosureModal.svelte'
	import MarketChart from '../../lib/MarketChart.svelte'
	import TickerIcon from '../../lib/TickerIcon.svelte'
	import { markets as fallbackMarkets, type Market } from '../../lib/market-data'
	import { getMarketRegistry } from '../../lib/markets'
	import { initializeWalletSession, walletSession, signWalletMessage } from '../../lib/wallet-core'
	import { showToast } from '../../lib/toasts'
	import {
		clearNotifications, deleteNotification, dollars, getAccountBalance, getHistory, getLimits,
		getMarketHistory, getMarketVolumes, getNotifications, getPositions, getSnapshots, postTrade,
		getAccountWithdrawalPayload, priceToRaw, submitAccountWithdrawal, usdc, usdcToRaw, walletPayload,
		type ActivityRecord, type LimitOrderRecord, type MarketSnapshot, type NotificationRecord,
		type PositionRecord, type PricePoint, type VolumePoint,
	} from '../../lib/dashboard'

	let selectedMarket = $state('NVDA')
	let sideOnEntry = $state<'long' | 'short'>('long')
	let markets = $state<Array<Market & { tradingEnabled?: boolean }>>(fallbackMarkets)
	let snapshots = $state<MarketSnapshot[]>([])
	let volumes = $state<Record<string, string>>({})
	let positions = $state<PositionRecord[]>([])
	let limitOrders = $state<LimitOrderRecord[]>([])
	let history = $state<ActivityRecord[]>([])
	let notifications = $state<NotificationRecord[]>([])
	let notificationsLoaded = false
	let points = $state<PricePoint[]>([])
	let volumePoints = $state<VolumePoint[]>([])
	let historyStartMs = $state<number | null>(null)
	let balance = $state<number | null>(null)
	let balanceRaw = $state<string | null>(null)
	let balanceModalOpen = $state(false)
	let chartRange = $state<'1h' | '1d' | '1w' | '1m' | 'all'>('1h')
	let activeTab = $state<'Positions' | 'Open orders' | 'History'>('Positions')
	let metricsLoaded = $state(false)
	let submitting = $state(false)
	let orderWidth = $state(342)
	let chartHeight = $state(485)
	let resizeDrag = $state<{ axis: 'x' | 'y'; start: number; initial: number } | null>(null)
	let estimatedFee = $state<number | null>(null)
	let marketSearch = $state('')
	let marketMenuOpen = $state(false)
	let notificationsOpen = $state(false)
	let riskOpen = $state(false)
	let apiLatency = $state<number | null>(null)
	let sharePosition = $state<{ ticker: string; side: 'Long' | 'Short'; size: string; leverage: string; entry: string; mark: string; pnl: string; change: string } | null>(null)
	let selectedMarketInfo = $derived(markets.find(item => item.ticker === selectedMarket) ?? markets[0])
	let snapshot = $derived(snapshots.find(item => item.market_id === selectedMarket))
	let markPrice = $derived(snapshot?.mark_price_e8 ? dollars(snapshot.mark_price_e8) : 0)
	let maxLeverage = $derived(snapshot?.max_leverage_bps ? Number(snapshot.max_leverage_bps) / 10_000 : 10)
	let tradingEnabled = $derived(Boolean(selectedMarketInfo?.tradingEnabled && snapshot?.enabled && !snapshot.close_only))
	let filteredMarkets = $derived(markets.filter(item => `${item.ticker} ${item.name}`.toLowerCase().includes(marketSearch.trim().toLowerCase())))

	let refreshTimer: number | undefined
	let latencyTimer: number | undefined
	let active = $state(false)
	let refreshInFlight = false
	let refreshPending = false

	onMount(() => {
		initializeWalletSession()
		const session = $walletSession
		if (!session) { void goto('/', { replaceState: true }); return }
		const params = new URLSearchParams(window.location.search)
		selectedMarket = params.get('market') ?? 'NVDA'
		sideOnEntry = params.get('side') === 'short' ? 'short' : 'long'
		active = true
		void getMarketRegistry().then(data => { if (active && data.markets.length) markets = data.markets }).catch(() => undefined)
		void refreshDashboard()
		refreshTimer = window.setInterval(() => {
			if (document.visibilityState === 'visible') void refreshDashboard()
		}, 30_000)
		const measureLatency = async () => {
			const controller = new AbortController(), timeout = window.setTimeout(() => controller.abort(), 5000), start = performance.now()
			try { await fetch(`/api/health/live?probe=${Date.now()}`, { cache: 'no-store', signal: controller.signal }); if (active) apiLatency = performance.now() - start }
			catch { if (active) apiLatency = null }
			finally { window.clearTimeout(timeout) }
		}
		void measureLatency()
		latencyTimer = window.setInterval(() => { if (document.visibilityState === 'visible') void measureLatency() }, 30_000)
		const onVisibility = () => { if (document.visibilityState === 'visible') void refreshDashboard() }
		document.addEventListener('visibilitychange', onVisibility)
		return () => {
			active = false
			if (refreshTimer) window.clearInterval(refreshTimer)
			if (latencyTimer) window.clearInterval(latencyTimer)
			document.removeEventListener('visibilitychange', onVisibility)
		}
	})

	async function refreshDashboard() {
		const session = $walletSession
		if (!session || !active || document.visibilityState === 'hidden') return
		if (refreshInFlight) { refreshPending = true; return }
		refreshInFlight = true
		const ticker = selectedMarket, range = chartRange, tab = activeTab
		const tasks = [
			getSnapshots().then(data => { if (active) { snapshots = data; metricsLoaded = true } }, () => { if (active) metricsLoaded = true }),
			getMarketVolumes().then(rows => { if (active) volumes = Object.fromEntries(rows.map(row => [row.market_id, row.volume_usdc_raw])) }),
			getMarketHistory(ticker, range).then(data => { if (active && selectedMarket === ticker && chartRange === range) { points = data.points ?? []; volumePoints = data.volume_points ?? []; historyStartMs = data.history_start_ms ?? null } }),
			getPositions(session.accountId).then(data => { if (active) positions = data.positions ?? [] }),
			getAccountBalance(session.accountId).then(data => { if (active) { balanceRaw = String(data.balance_usdc); balance = usdc(balanceRaw) } }),
			...(!notificationsLoaded || notificationsOpen ? [getNotifications(session.accountId).then(data => { if (active) { notifications = data.notifications ?? []; notificationsLoaded = true } })] : []),
			...(tab === 'Open orders' ? [getLimits(session.accountId).then(data => { if (active) limitOrders = data.orders ?? [] })] : []),
			...(tab === 'History' ? [getHistory(session.accountId).then(data => { if (active) history = data.events ?? [] })] : []),
		]
		await Promise.allSettled(tasks)
		refreshInFlight = false
		if (refreshPending) { refreshPending = false; window.setTimeout(() => void refreshDashboard(), 0) }
	}

	async function submitOrder(order: { side: 'long' | 'short'; orderType: 'market' | 'limit'; margin: string; notional: string; leverage: number; limitPrice: string }) {
		const session = $walletSession
		if (!session || submitting) return
		submitting = true
		try {
			const notionalRaw = String(Math.round(Number(order.notional) * 1e6))
			if (order.orderType === 'limit') {
				const draft = { account_id: session.accountId, order_id: `o_${crypto.randomUUID().replaceAll('-', '').slice(0, 24)}`, market_id: selectedMarket, side: order.side, notional_usdc: notionalRaw, leverage_bps: Math.round(order.leverage * 10_000), limit_price: priceToRaw(Number(order.limitPrice)), expires_at_ms: Date.now() + 86_400_000 }
				const quote = await postTrade<Record<string, any>>('/v1/trade/limits/quote', draft)
				if (quote.fee_usdc != null) estimatedFee = usdc(quote.fee_usdc)
				const body = { ...draft, ...quote }
				const signature = await signWalletMessage(walletPayload(String(quote.message), quote.nonce, quoteRecipient(quote.recipient)))
				await postTrade('/v1/trade/limits', { ...body, owner_signature: signature })
			} else {
				const base = { account_id: session.accountId, market_id: selectedMarket, side: order.side, notional_usdc: notionalRaw, leverage_bps: Math.round(order.leverage * 10_000), max_slippage_bps: 100 }
				const quote = await postTrade<Record<string, any>>('/v1/trade/quote', base)
				estimatedFee = usdc(quote.fee_usdc)
				const signature = await signWalletMessage(walletPayload(String(quote.message), quote.nonce, quoteRecipient(quote.recipient)))
				await postTrade('/v1/trade/open', { ...quote, owner_signature: signature })
			}
			showToast('success', `${order.side === 'long' ? 'Long' : 'Short'} order submitted.`)
			void refreshDashboard()
		} catch (reason) {
			showToast('error', reason instanceof Error ? reason.message : 'Order could not be submitted.')
		} finally { submitting = false }
	}

	async function closePosition(position: PositionRecord) {
		const session = $walletSession
		if (!session || submitting) return
		submitting = true
		try {
			const quote = await postTrade<Record<string, any>>('/v1/trade/close/quote', { account_id: session.accountId, position_id: position.position_id, max_slippage_bps: 100 })
			const signature = await signWalletMessage(walletPayload(String(quote.message), quote.nonce, quoteRecipient(quote.recipient)))
			await postTrade('/v1/trade/close', { ...quote, market_id: position.market_id, side: position.is_long ? 'long' : 'short', notional_usdc: position.notional_usdc, owner_signature: signature })
			showToast('success', `${position.market_id} position closed.`)
			void refreshDashboard()
		} catch (reason) { showToast('error', reason instanceof Error ? reason.message : 'Position could not be closed.') }
		finally { submitting = false }
	}
	async function cancelLimit(order: LimitOrderRecord) {
		const session = $walletSession
		if (!session || submitting) return
		submitting = true
		try {
			const quote = await postTrade<Record<string, any>>('/v1/trade/limits/cancel/quote', { account_id: session.accountId, order_id: order.order_id })
			const body = { ...quote, account_id: session.accountId, order_id: order.order_id, market_id: order.market_id, side: order.is_long ? 'long' : 'short', notional_usdc: order.notional_usdc }
			const signature = await signWalletMessage(walletPayload(String(quote.message), quote.nonce, quoteRecipient(quote.recipient)))
			await postTrade('/v1/trade/limits/cancel', { ...body, owner_signature: signature })
			showToast('success', 'Limit order cancelled.')
			void refreshDashboard()
		} catch (reason) { showToast('error', reason instanceof Error ? reason.message : 'Limit order could not be cancelled.') }
		finally { submitting = false }
	}
	async function clearAllNotifications() {
		const session = $walletSession
		if (!session) return
		try { await clearNotifications(session.accountId); notifications = []; notificationsLoaded = true }
		catch (reason) { showToast('error', reason instanceof Error ? reason.message : 'Could not clear notifications.') }
	}
	async function removeNotification(item: NotificationRecord) {
		const session = $walletSession
		if (!session) return
		try { await deleteNotification(session.accountId, item.id); notifications = notifications.filter(row => row.id !== item.id); notificationsLoaded = true }
		catch (reason) { showToast('error', reason instanceof Error ? reason.message : 'Could not delete notification.') }
	}
	async function withdrawFromAccount(amount: string, receiver: string) {
		const session = $walletSession
		if (!session) throw new Error('Connect a wallet before withdrawing.')
		const payload = await getAccountWithdrawalPayload(session.accountId, receiver, usdcToRaw(amount))
		const signature = await signWalletMessage(walletPayload(payload.message, payload.nonce, payload.recipient))
		const result = await submitAccountWithdrawal({ ...payload, signature })
		showToast('success', `Withdrawal submitted. Transaction ${result.transaction_hash.slice(0, 8)}…`)
		await refreshDashboard()
	}

	function money(value: number) { return `$${value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` }
	function quoteRecipient(recipient?: string) { return recipient || import.meta.env.VITE_A1_CONTRACT_ID || (import.meta.env.VITE_NEAR_NETWORK === 'mainnet' ? 'a1.afterbell.near' : 'afterbellfx-dev-faucet.testnet') }
	function formatElapsed(timestamp: number) { const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000)); return minutes < 1 ? 'Just now' : minutes < 60 ? `${minutes}m ago` : minutes < 1440 ? `${Math.floor(minutes / 60)}h ago` : `${Math.floor(minutes / 1440)}d ago` }
	let latencyGrade = $derived(apiLatency === null ? 'measuring' : apiLatency <= 100 ? 'good' : apiLatency <= 250 ? 'medium' : 'poor')
	async function copyAddress() { try { if ($walletSession) await navigator.clipboard.writeText($walletSession.address); showToast('success', 'Wallet address copied.') } catch { showToast('error', 'Could not copy the wallet address.') } }
	function navigateMarket(ticker: string) { selectedMarket = ticker; metricsLoaded = false; marketMenuOpen = false; marketSearch = ''; void goto(`/trade?market=${encodeURIComponent(ticker)}&side=${sideOnEntry}`, { replaceState: true }); void refreshDashboard() }
	function selectTab(tab: string) { activeTab = tab as 'Positions' | 'Open orders' | 'History'; void refreshDashboard() }
	function toggleNotifications() { notificationsOpen = !notificationsOpen; if (notificationsOpen) void refreshDashboard() }
	function selectChartRange(range: '1h' | '1d' | '1w' | '1m' | 'all') {
		const requiredMs: Record<typeof range, number> = { '1h': 3_600_000, '1d': 86_400_000, '1w': 604_800_000, '1m': 2_592_000_000, all: 2_592_000_000 }
		if (historyStartMs === null || Date.now() - historyStartMs < requiredMs[range]) {
			showToast('waiting', 'We’re still collecting enough market history for that chart range.')
			return
		}
		chartRange = range
		void refreshDashboard()
	}
	function startResize(axis: 'x' | 'y', event: PointerEvent) {
		const target = event.currentTarget as HTMLButtonElement
		target.setPointerCapture(event.pointerId)
		resizeDrag = { axis, start: axis === 'x' ? event.clientX : event.clientY, initial: axis === 'x' ? orderWidth : chartHeight }
	}
	function moveResize(event: PointerEvent) {
		if (!resizeDrag) return
		const delta = resizeDrag.axis === 'x' ? event.clientX - resizeDrag.start : event.clientY - resizeDrag.start
		if (resizeDrag.axis === 'x') orderWidth = Math.min(480, Math.max(290, resizeDrag.initial - delta))
		else chartHeight = Math.min(720, Math.max(300, resizeDrag.initial + delta))
	}
	function endResize() { resizeDrag = null }
</script>

<svelte:head>
	<title>Trade {selectedMarket} | Afterbell</title>
	<meta name="description" content="Trade cash-settled {selectedMarket} perpetuals on Afterbell." />
</svelte:head>

{#if $walletSession}
<div class="trade-app">
	<DashboardHeader active="Trade" {balance} notificationCount={notifications.length} onBalanceClick={() => balanceModalOpen = true} onNotificationsClick={toggleNotifications}/>

	<main class="trade-main">
		<section class="market-overview" aria-label="Market overview">
			<div class="market-selector-wrap">
				<button class="market-selector" aria-expanded={marketMenuOpen} onclick={() => marketMenuOpen = !marketMenuOpen}><span class="stock-avatar"><TickerIcon ticker={selectedMarket} size={20}/></span><span class="market-selector-text"><b>{selectedMarket}</b><small>{selectedMarketInfo?.name ?? 'US equity'}</small></span><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="m7 10 5 5 5-5"/></svg></button>
				{#if marketMenuOpen}<div class="market-menu"><label class="market-search"><svg viewBox="0 0 24 24" fill="none"><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></svg><input bind:value={marketSearch} placeholder="Search stocks" aria-label="Search stocks"/></label><div class="market-menu-list">{#each filteredMarkets as item (item.ticker)}<button class:selected={item.ticker === selectedMarket} onclick={() => navigateMarket(item.ticker)}><span class="stock-avatar small"><TickerIcon ticker={item.ticker} size={17}/></span><span><b>{item.ticker}</b><small>{item.name} / US equity</small></span></button>{/each}{#if !filteredMarkets.length}<p class="market-empty">No stocks found.</p>{/if}</div></div>{/if}
			</div>
			<div class="overview-price"><small class="metric-label" aria-describedby="tip-mark-price">Perp mark price<span class="metric-tooltip" id="tip-mark-price" role="tooltip">The current market mark used for position valuation and risk checks.</span></small><strong>{#if !metricsLoaded}<span class="metric-skeleton"></span>{:else}{markPrice ? money(markPrice) : 'Unavailable'}{/if}</strong><span>{#if !metricsLoaded}<span class="metric-skeleton metric-skeleton-small"></span>{:else}{snapshot?.observed_at_ms ? `Updated ${formatElapsed(snapshot.observed_at_ms)}` : 'Waiting for oracle price'}{/if}</span></div>
			<div class="overview-metric"><small class="metric-label" aria-describedby="tip-volume">24h volume<span class="metric-tooltip" id="tip-volume" role="tooltip">Notional value of recorded opens and closes in the last 24 hours.</span></small><strong>{#if !metricsLoaded}<span class="metric-skeleton"></span>{:else}{volumes[selectedMarket] ? money(usdc(volumes[selectedMarket])) : '$0.00'}{/if}</strong></div>
			<div class="overview-metric"><small class="metric-label" aria-describedby="tip-open-interest">Open interest<span class="metric-tooltip" id="tip-open-interest" role="tooltip">Combined open long and short notional on A1.</span></small><strong>{#if !metricsLoaded}<span class="metric-skeleton"></span>{:else}{snapshot ? money(usdc(snapshot.open_interest_usdc)) : 'Unavailable'}{/if}</strong></div>
			<div class="overview-metric funding"><small class="metric-label" aria-describedby="tip-funding">Funding / 8h<span class="metric-tooltip" id="tip-funding" role="tooltip">Current funding rate projected across eight hours.</span></small><strong>{#if !metricsLoaded}<span class="metric-skeleton"></span>{:else}{snapshot ? `${(snapshot.funding_rate_bps_8h / 100).toFixed(4)}%` : 'Unavailable'}{/if}</strong></div>
			<div class="stock-quote"><small>{selectedMarket}-USDC</small><strong>{#if !metricsLoaded}<span class="metric-skeleton"></span>{:else}{markPrice ? money(markPrice) : 'Unavailable'}{/if}</strong></div>
		</section>

		<div class="trading-grid" style={`--order-width:${orderWidth}px;--chart-height:${chartHeight}px`}>
			<section class="market-workspace" aria-label="Market chart and positions">
				<MarketChart ticker={selectedMarket} name={selectedMarketInfo?.name ?? selectedMarket} {points} {volumePoints} range={chartRange} onRangeChange={selectChartRange}/>
				<button class="resize-handle resize-horizontal" aria-label="Resize chart and positions panels" onpointerdown={event => startResize('y', event)} onpointermove={moveResize} onpointerup={endResize} onpointercancel={endResize}><span></span></button>
				<div class="positions-panel">
					<div class="positions-heading"><div class="position-tabs">{#each ['Positions', 'Open orders', 'History'] as tab (tab)}<button class:active={activeTab === tab} onclick={() => selectTab(tab)}>{tab}{#if tab === 'Positions'}<span class="tab-count">{positions.length}</span>{/if}</button>{/each}</div></div>
					<div class="position-table-wrap"><table class="position-table"><thead><tr><th>Market</th><th>Side</th><th>Size</th><th>{activeTab === 'Open orders' ? 'Limit price' : 'Margin'}</th><th>{activeTab === 'History' ? 'Event' : 'Leverage'}</th><th>{activeTab === 'History' ? 'Time' : 'Entry price'}</th><th>{activeTab === 'History' ? 'Transaction' : 'Mark price'}</th><th>{activeTab === 'History' ? 'Status' : 'Unrealized PnL'}</th><th></th></tr></thead><tbody>
						{#if activeTab === 'Positions'}
							{#each positions as position (position.position_id)}
								{@const notional = usdc(position.notional_usdc)}
								{@const pnl = usdc(position.unrealized_pnl_usdc)}
								{@const mark = dollars(position.mark_price)}
								<tr><td><span class="table-market"><span class="stock-avatar tiny"><TickerIcon ticker={position.market_id} size={13}/></span><b>{position.market_id}</b><small>USDC</small></span></td><td><span class:long-side={position.is_long} class:short-side={!position.is_long}>{position.is_long ? '↗ Long' : '↘ Short'}</span></td><td>{money(notional)}</td><td>{money(usdc(position.margin_usdc ?? String(Math.ceil(Number(position.notional_usdc) * 10_000 / position.leverage_bps))))}</td><td>{(position.leverage_bps / 10_000).toFixed(1)}×</td><td>{money(dollars(position.entry_price))}</td><td>{mark ? money(mark) : 'Unavailable'}</td><td><span class:green-pnl={pnl >= 0} class:short-side={pnl < 0}>{pnl >= 0 ? '+' : ''}{money(pnl)}</span></td><td><div class="position-actions"><button class="share-position" aria-label={`Share ${position.market_id} trade`} onclick={() => { sharePosition = { ticker: position.market_id, side: position.is_long ? 'Long' : 'Short', size: money(notional), leverage: `${(position.leverage_bps / 10_000).toFixed(1)}×`, entry: money(dollars(position.entry_price)), mark: mark ? money(mark) : 'Unavailable', pnl: `${pnl >= 0 ? '+' : ''}${money(pnl)}`, change: `${notional ? (pnl / notional * 100).toFixed(2) : '0.00'}%` } }}>Share</button><button class="close-position" disabled={submitting} onclick={() => void closePosition(position)}>Close</button></div></td></tr>
							{/each}
						{:else if activeTab === 'Open orders'}
							{#each limitOrders as order (order.order_id)}<tr><td><span class="table-market"><span class="stock-avatar tiny"><TickerIcon ticker={order.market_id} size={13}/></span><b>{order.market_id}</b><small>USDC</small></span></td><td>{order.is_long ? 'Long' : 'Short'}</td><td>{money(usdc(order.notional_usdc))}</td><td>{money(dollars(order.limit_price))}</td><td>{(order.leverage_bps / 10_000).toFixed(1)}×</td><td>Limit</td><td>—</td><td>Pending</td><td><button class="close-position" disabled={submitting} onclick={() => void cancelLimit(order)}>Cancel</button></td></tr>{/each}
						{:else}
							{#each history as item (item.id)}<tr><td><span class="table-market"><span class="stock-avatar tiny">{#if item.market_id}<TickerIcon ticker={item.market_id} size={13}/>{/if}</span><b>{item.market_id || 'Account'}</b><small>USDC</small></span></td><td>{item.side || '—'}</td><td>{money(usdc(item.amount_usdc))}</td><td>—</td><td>{item.event_type.replaceAll('_', ' ')}</td><td>{formatElapsed(item.created_at_ms)}</td><td>{item.transaction_hash ? `${item.transaction_hash.slice(0, 8)}…` : '—'}</td><td>Finalized</td><td></td></tr>{/each}
						{/if}
					</tbody></table></div>
					{#if (activeTab === 'Positions' && positions.length === 0) || (activeTab === 'Open orders' && limitOrders.length === 0) || (activeTab === 'History' && history.length === 0)}<div class="position-empty">{activeTab === 'Positions' ? 'No open positions.' : activeTab === 'Open orders' ? 'No open orders.' : 'No account activity yet.'}</div>{/if}
				</div>
			</section>
			<button class="resize-handle resize-vertical" aria-label="Resize order panel" onpointerdown={event => startResize('x', event)} onpointermove={moveResize} onpointerup={endResize} onpointercancel={endResize}><span></span></button>
			<OrderPanel marketId={selectedMarket} initialSide={sideOnEntry} availableBalance={balance} maxLeverage={Math.max(1, maxLeverage)} entryPrice={markPrice || null} tradingEnabled={tradingEnabled} {submitting} {estimatedFee} onSubmit={submitOrder}/>
		</div>
	</main>
	<footer class="trade-statusbar"><span class={`interface-speed ${latencyGrade}`}>{apiLatency === null ? '-- ms' : `${Math.round(apiLatency)} ms`}</span><span class="status-spacer"></span><button class="footer-link" onclick={() => riskOpen = true}>Risk disclosure</button><a class="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
	{#if riskOpen}<RiskDisclosureModal onClose={() => riskOpen = false}/>{/if}
	{#if notificationsOpen}<button class="drawer-backdrop" aria-label="Close notifications" onclick={() => notificationsOpen = false}></button><aside class="notification-drawer" aria-label="Notifications"><div class="drawer-heading"><div><h2>Notifications</h2><p>Recent account activity</p></div><button class="drawer-close" aria-label="Close notifications" onclick={() => notificationsOpen = false}>×</button></div><div class="drawer-actions"><span>{notifications.length} {notifications.length === 1 ? 'notification' : 'notifications'}</span><button disabled={!notifications.length} onclick={() => void clearAllNotifications()}>Clear all</button></div><div class="notification-list">{#each notifications as item (item.id)}<article class="notification-item"><div class="notification-body"><div class="notification-title-row"><h3>{item.title}</h3><time>{formatElapsed(item.created_at_ms)}</time></div><p>{item.message}</p></div><button class="notification-delete" aria-label={`Delete ${item.title}`} onclick={() => void removeNotification(item)}>×</button></article>{/each}{#if notifications.length === 0}<div class="notifications-empty"><span>✓</span><h3>All clear</h3><p>You have no notifications.</p></div>{/if}</div></aside>{/if}
	{#if balanceModalOpen && $walletSession}<AccountBalanceModal session={$walletSession} {balanceRaw} onClose={() => balanceModalOpen = false} onCopy={async value => { try { await navigator.clipboard.writeText(value); showToast('success', 'Copied to clipboard.') } catch { showToast('error', 'Could not copy to clipboard.') } }} onWithdraw={withdrawFromAccount} onDepositComplete={refreshDashboard}/>{/if}
	{#if sharePosition}<ShareTradeModal position={sharePosition} onClose={() => sharePosition = null}/>{/if}
</div>
{/if}
