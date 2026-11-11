import { createFileRoute } from '@tanstack/react-router'
import { useState, useRef, useEffect, useMemo, type CSSProperties, type PointerEvent as ReactPointerEvent, type ChangeEvent } from 'react'
import { Candlesticks, Chart, Panel } from 'react-candlesticks'
import type { ChartHandle, DataPoint, Theme } from 'react-candlesticks'
import 'react-candlesticks/style.css'
import { AccountBalanceModal } from '../components/AccountBalanceModal'
import logo from '../assets/afterbell-logo.png'
import shareImage1 from '../assets/share-images/1.png'
import shareImage2 from '../assets/share-images/2.png'
import shareImage3 from '../assets/share-images/3.png'
import { TickerIcon } from '../components/TickerIcon'
import { UsdcIcon } from '../components/UsdcIcon'
import { WalletDetailsModal } from '../components/WalletDetailsModal'
import { markets, type Market } from '../data/markets'
import { useMarketRegistry } from '../lib/api'
import { useWalletSession } from '../lib/wallet-session'
import { getChainName, getWalletIcon } from '../lib/wallet-icons'
import { clearNotifications, deleteNotification, dollars, getAccountWithdrawalPayload, getDepositInstructions, getHistory, getLimits, getMarketHistory, getMarketVolumes, getNearWalletBalance, getNotifications, getPositions, getSnapshots, postTrade, priceToRaw, submitAccountWithdrawal, usdc, usdcToRaw, walletPayload, type ActivityRecord, type LimitOrderRecord, type MarketSnapshot, type NotificationRecord, type PositionRecord, type PricePoint } from '../lib/dashboard'
import { useToasts } from '../lib/toasts'
import './trade.css'

export const Route = createFileRoute('/trade')({
  validateSearch: (search: Record<string, unknown>): { market: string | undefined; side: 'long' | 'short' | undefined } => ({
    market: typeof search.market === 'string' ? search.market : undefined,
    side: search.side === 'long' || search.side === 'short' ? search.side : undefined,
  }),
  component: TradingPage,
})

function Icon({ name }: { name: 'chart' | 'chevron' | 'bell' | 'search' }) {
  const paths = {
    chart: <><path d="M4 19V5M4 19h17"/><path d="m7 15 4-5 3 2 6-7"/></>,
    chevron: <path d="m7 10 5 5 5-5"/>,
    bell: <><path d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"/><path d="M10 21h4"/></>,
    search: <><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></>,
  }
  return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>
}

function loadCanvasImage(source: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = reject
    image.src = source
  })
}

function loadCanvasSvg(svg: SVGSVGElement) {
  const clone = svg.cloneNode(true) as SVGSVGElement
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg')
  const url = URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(clone)], { type: 'image/svg+xml;charset=utf-8' }))
  return loadCanvasImage(url).finally(() => URL.revokeObjectURL(url))
}

const chartRanges = ['1h', '1d', '1w', '1m', 'all'] as const
type ChartRange = typeof chartRanges[number]
const candleIntervalMs: Record<ChartRange, number> = { '1h': 60_000, '1d': 300_000, '1w': 900_000, '1m': 14_400_000, all: 86_400_000 }
const chartGranularity: Record<ChartRange, 'm1' | 'm5' | 'm15' | 'h4' | 'd1'> = { '1h': 'm1', '1d': 'm5', '1w': 'm15', '1m': 'h4', all: 'd1' }
const chartTheme: Theme = {
  base: 'dark',
  chart: {
    backgroundColor: '#0c1016',
    grid: {
      time: { color: '#10151c', width: 1 },
      value: { color: '#151c25', width: 1 },
    },
    borders: {
      left: { color: '#0c1016', width: 0 },
      right: { color: '#202a36', width: 1 },
      top: { color: '#0c1016', width: 0 },
      bottom: { color: '#202a36', width: 1 },
    },
  },
  layers: {
    candlesticks: {
      series: {
        body: {
          up: { backgroundColor: '#39c98a', borderColor: '#39c98a' },
          down: { backgroundColor: '#e45b68', borderColor: '#e45b68' },
        },
        wick: {
          up: { color: '#39c98a' },
          down: { color: '#e45b68' },
        },
      },
    },
  },
}

function PriceChart({ market, points, range, onRangeChange }: { market: Market; points: PricePoint[]; range: ChartRange; onRangeChange: (range: ChartRange) => void }) {
  const chartRef = useRef<ChartHandle>(null)
  const fittedRef = useRef<{ ticker: string; range: ChartRange } | null>(null)
  const candles = useMemo(() => {
    const buckets = new Map<number, { open: number; high: number; low: number; close: number }>()
    const interval = candleIntervalMs[range]
    for (const point of points) {
      const value = dollars(point.price_e8)
      if (!Number.isFinite(value) || value <= 0) continue
      const bucket = Math.floor(point.timestamp_ms / interval) * interval
      const current = buckets.get(bucket)
      if (current) { current.high = Math.max(current.high, value); current.low = Math.min(current.low, value); current.close = value }
      else buckets.set(bucket, { open: value, high: value, low: value, close: value })
    }
    return Array.from(buckets, ([timestamp, candle]) => ({ time: new Date(timestamp).toISOString(), volume: 0, ...candle }))
      .sort((left, right) => left.time.localeCompare(right.time)) as DataPoint[]
  }, [points, range])

  useEffect(() => {
    if (!candles.length || !chartRef.current) return
    if (fittedRef.current?.ticker === market.ticker && fittedRef.current.range === range) return
    fittedRef.current = { ticker: market.ticker, range }
    const frame = requestAnimationFrame(() => chartRef.current?.fitContent({ marginBars: 4 }))
    return () => cancelAnimationFrame(frame)
  }, [candles, market.ticker, range])

  const latest = candles[candles.length - 1]
  return <div className="chart-shell">
    <div className="chart-tools">
      <div className="chart-pair"><span>{market.ticker} / USDC</span></div>
      <div className="chart-actions" role="group" aria-label="Chart time range">{chartRanges.map(item => <button key={item} className={`chart-range-button ${range === item ? 'active' : ''}`} onClick={() => onRangeChange(item)}>{item.toUpperCase()}</button>)}</div>
    </div>
    <div className="real-market-chart" role="img" aria-label={`${market.name} candlestick chart`}>
      <Chart ref={chartRef} className="oracle-candlestick-chart" width="auto" height="auto" data={candles} granularity={chartGranularity[range]} backgroundColor="#0c1016" theme={chartTheme} initialScrollToLatest enableScroll enableZoom>
        <Panel><Candlesticks legend={false} /></Panel>
      </Chart>
      {candles.length === 0 && <div className="chart-placeholder">Candles will appear as oracle prices arrive.</div>}
      {latest && <div className="chart-ohlc" aria-label="Latest candle"><span>O <b>{latest.open.toFixed(2)}</b></span><span>H <b>{latest.high.toFixed(2)}</b></span><span>L <b>{latest.low.toFixed(2)}</b></span><span>C <b>{latest.close.toFixed(2)}</b></span></div>}
    </div>
  </div>
}

type PerformanceState = { ms: number | null }

function InterfaceSpeed() {
  const [speed, setSpeed] = useState<PerformanceState>({ ms: null })

  useEffect(() => {
    let active = true
    const apiBase = (import.meta.env.VITE_API_BASE_URL || 'https://api.afterbellfx.xyz').replace(/\/$/, '')
    const measureNetwork = async () => {
      const controller = new AbortController()
      const timeout = window.setTimeout(() => controller.abort(), 5000)
      const startedAt = performance.now()
      try {
        await fetch(`${apiBase}/health/live?probe=${Date.now()}`, {
          method: 'GET',
          mode: 'no-cors',
          cache: 'no-store',
          signal: controller.signal,
        })
        if (active) setSpeed({ ms: performance.now() - startedAt })
      } catch {
        if (active) setSpeed({ ms: null })
      } finally {
        window.clearTimeout(timeout)
      }
    }
    void measureNetwork()
    const interval = window.setInterval(() => void measureNetwork(), 30000)
    return () => {
      active = false
      window.clearInterval(interval)
    }
  }, [])

  const grade = speed.ms === null ? 'measuring' : speed.ms <= 100 ? 'good' : speed.ms <= 250 ? 'medium' : 'poor'
  const networkReadout = speed.ms !== null ? `${Math.round(speed.ms)} ms` : '-- ms'
  return <span className={`interface-speed ${grade}`} title="Round-trip request time to the Afterbell API">{networkReadout}</span>
}

const riskDisclosureSections = [
  { title: 'Synthetic equity exposure', body: 'Afterbell markets are leveraged, cash-settled perpetual contracts that reference selected US equities. A contract is not a share and gives no ownership, voting, dividend, distribution, or corporate-action rights in the referenced company. Prices can diverge from the underlying equity, particularly outside regular US market hours.' },
  { title: 'Leverage, collateral and liquidation', body: 'Leverage increases both gains and losses. Positions use shared USDC collateral and can be liquidated when their equity falls below the required maintenance margin. Liquidation can happen quickly, at an unfavorable price, and may consume all collateral allocated to a position. Fees, funding payments, slippage, and market impact reduce returns.' },
  { title: 'Oracle prices and trade execution', body: 'Displayed marks and execution quotes depend on external Robinhood Chain market data, an oracle publisher, NEAR contract state, and backend relaying and keeper services. Data can be delayed, unavailable, wrong, or manipulated. Freshness limits, deviation checks, circuit breakers, and signed quotes reduce some risks but cannot guarantee a correct price or successful execution. An oracle signer, quote signer, RPC provider, or service compromise may cause loss. Orders may fail, remain unfilled, or execute differently from an expected reference price.' },
  { title: 'Shared vault and liquidity providers', body: 'The shared liquidity vault is the counterparty to positions. Vault liquidity providers bear market, utilization, oracle, liquidation, bad-debt, stablecoin, and contract risks. Vault shares are not bank deposits and have no guaranteed value. Withdrawals may be restricted or delayed by available liquidity and protocol rules, and losses can reduce vault assets and share value.' },
  { title: 'Wallet ownership, agents and transactions', body: 'Your wallet controls your Afterbell account and authorizes account actions. Review each signature, transaction, and agent permission. Agent accounts are logical records with limited permissions, but an agent can still lose funds within those limits. You are responsible for monitoring agent activity and revoking access when appropriate. Blockchain actions may be irreversible. Contract defects, compromised keys, protocol administration, chain outages, congestion, RPC failures, wallet errors, and other service interruptions can delay or prevent actions or result in loss.' },
  { title: 'USDC and cross-chain transfers', body: 'USDC is a digital asset, not a bank balance, and may lose its peg or be affected by issuer, reserve, token-contract, bridge, or redemption events. Deposits and withdrawals that use cross-chain services depend on NEAR Intents, solvers, and external networks; they can be delayed, fail, or be refunded under those services’ processes. Check the asset, network, and destination before confirming a transfer.' },
  { title: 'No assurance of profit or availability', body: 'Trading can result in partial or total loss and may not be suitable for you. Do not use money you cannot afford to lose. Past performance does not predict future results. No market, order type, risk control, agent, or protocol feature guarantees profit or prevents loss. Service and market availability can change, and use is subject to applicable law and geographic restrictions.' },
]

type SharedPosition = { ticker: string; side: 'Long' | 'Short'; size: string; margin: string; leverage: string; entry: string; mark: string; pnl: string; change: string }

function money(value: number) { return `$${value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` }
function elapsed(timestamp: number) { const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000)); return minutes < 1 ? 'Just now' : minutes < 60 ? `${minutes}m ago` : minutes < 1440 ? `${Math.floor(minutes / 60)}h ago` : `${Math.floor(minutes / 1440)}d ago` }

const shareBackgrounds = [
  { id: 'gradient-1', label: 'Blue glow', kind: 'gradient' },
  { id: 'gradient-2', label: 'Violet glow', kind: 'gradient' },
  { id: 'gradient-3', label: 'Emerald glow', kind: 'gradient' },
  { id: 'gradient-4', label: 'Warm glow', kind: 'gradient' },
  { id: 'image-1', label: 'Image 1', kind: 'image', image: shareImage1 },
  { id: 'image-2', label: 'Image 2', kind: 'image', image: shareImage2 },
  { id: 'image-3', label: 'Image 3', kind: 'image', image: shareImage3 },
]

function TradingPage() {
  const { session, disconnect, signMessage } = useWalletSession()
  const { showToast } = useToasts()
  useEffect(() => {
    if (!session) window.location.assign('/')
  }, [session])
  const { market: selectedTicker, side: initialSide } = Route.useSearch()
  const [selectedMarket, setSelectedMarket] = useState(selectedTicker ?? markets[0].ticker)
  const { markets: availableMarkets } = useMarketRegistry()
  const [side, setSide] = useState<'long' | 'short'>(initialSide ?? 'long')
  const [orderType, setOrderType] = useState<'market' | 'limit'>('market')
  const [leverage, setLeverage] = useState(3)
  const [size, setSize] = useState('250')
  const [payment, setPayment] = useState('100')
  const [activeTab, setActiveTab] = useState('Positions')
  const [marketMenu, setMarketMenu] = useState(false)
  const [marketSearch, setMarketSearch] = useState('')
  const [notificationsOpen, setNotificationsOpen] = useState(false)
  const [notifications, setNotifications] = useState<NotificationRecord[]>([])
  const [positions, setPositions] = useState<PositionRecord[]>([])
  const [limitOrders, setLimitOrders] = useState<LimitOrderRecord[]>([])
  const [history, setHistory] = useState<ActivityRecord[]>([])
  const [pricePoints, setPricePoints] = useState<PricePoint[]>([])
  const [snapshots, setSnapshots] = useState<MarketSnapshot[]>([])
  const [volumes, setVolumes] = useState<Record<string, string>>({})
  const [chartRange, setChartRange] = useState<ChartRange>('1d')
  const [metricsLoadedMarket, setMetricsLoadedMarket] = useState<string | null>(null)
  const [availableBalance, setAvailableBalance] = useState<number | null>(null)
  const [balanceRaw, setBalanceRaw] = useState<string | null>(null)
  const [nearWalletBalanceRaw, setNearWalletBalanceRaw] = useState<string | null>(null)
  const [depositInstructions, setDepositInstructions] = useState<{ contract_id: string; token_id: string; network: string } | null>(null)
  const [balanceModalOpen, setBalanceModalOpen] = useState(false)
  const [walletModalOpen, setWalletModalOpen] = useState(false)
  const [limitPrice, setLimitPrice] = useState('')
  const [estimatedFee, setEstimatedFee] = useState<number | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [riskOpen, setRiskOpen] = useState(false)
  const [sharePosition, setSharePosition] = useState<SharedPosition | null>(null)
  const [shareBackground, setShareBackground] = useState('gradient-1')
  const [customShareBackground, setCustomShareBackground] = useState<string | null>(null)
  const shareCardRef = useRef<HTMLElement>(null)
  const metricsRequestMarketRef = useRef<string | null>(null)
  const [shareNotice, setShareNotice] = useState('')
  const [notice, setNotice] = useState('')
  const [orderWidth, setOrderWidth] = useState(342)
  const [chartHeight, setChartHeight] = useState(485)
  const marketMenuRef = useRef<HTMLDivElement>(null)
  const dragState = useRef<{ axis: 'x' | 'y'; start: number; initial: number } | null>(null)
  const market = availableMarkets.find(item => item.ticker === selectedMarket) ?? availableMarkets[0]
  const filteredMarkets = availableMarkets.filter(item => `${item.ticker} ${item.name}`.toLowerCase().includes(marketSearch.trim().toLowerCase()))
  const snapshot = snapshots.find(item => item.market_id === market.ticker)
  const latestPrice = snapshot?.mark_price_e8 ? dollars(snapshot.mark_price_e8) : 0
  const maxLeverage = snapshot?.max_leverage_bps ? Number(snapshot.max_leverage_bps) / 10_000 : 10
  const tradingEnabled = Boolean(availableMarkets.find(item => item.ticker === market.ticker)?.tradingEnabled && snapshot?.enabled && !snapshot.close_only)

  useEffect(() => {
    if (snapshot?.max_leverage_bps && leverage > maxLeverage) {
      setLeverage(maxLeverage)
      setSize((Number(payment || 0) * maxLeverage).toFixed(2))
    }
  }, [snapshot?.max_leverage_bps, maxLeverage])

  async function refreshDashboard() {
    if (!session) return
    if (metricsRequestMarketRef.current !== market.ticker) {
      metricsRequestMarketRef.current = market.ticker
      setMetricsLoadedMarket(null)
    }
    const tasks = [
      getSnapshots().then(data => setSnapshots(data)),
      getMarketVolumes().then(rows => setVolumes(Object.fromEntries(rows.map(row => [row.market_id, row.volume_usdc_raw])))),
      getMarketHistory(market.ticker, chartRange).then(data => setPricePoints(data.points)),
      getPositions(session.accountId).then(data => setPositions(data.positions)),
      getLimits(session.accountId).then(data => setLimitOrders(data.orders)),
      getHistory(session.accountId).then(data => setHistory(data.events)),
      getNotifications(session.accountId).then(data => setNotifications(data.notifications)),
      fetch(`/api/v1/account/balance?account_id=${encodeURIComponent(session.accountId)}`).then(response => response.ok ? response.json() : Promise.reject()).then(data => { const raw = String(data.balance_usdc); setBalanceRaw(raw); setAvailableBalance(usdc(raw)) }),
      getDepositInstructions(session.accountId).then(setDepositInstructions),
      session.chain === 'near' ? getNearWalletBalance(session.address).then(data => setNearWalletBalanceRaw(data.amount_yocto)) : Promise.resolve(),
    ]
    await Promise.allSettled(tasks)
    if (metricsRequestMarketRef.current === market.ticker) setMetricsLoadedMarket(market.ticker)
  }
  useEffect(() => { void refreshDashboard(); const timer = window.setInterval(() => void refreshDashboard(), 15000); return () => window.clearInterval(timer) }, [session?.accountId, market.ticker, chartRange])

  const walletIcon = session ? getWalletIcon(session.walletName) : undefined
  const chainName = session ? getChainName(session.chain) : 'NEAR'

  async function copyWalletAddress(address: string) {
    try { await navigator.clipboard.writeText(address); showToast('success', 'Wallet address copied.') }
    catch { showToast('error', 'Could not copy the wallet address.') }
  }

  async function disconnectWallet() {
    await disconnect()
    window.location.assign('/')
  }

  async function withdrawFromAccount(amount: string, receiver: string) {
    if (!session) throw new Error('Connect your wallet first.')
    const amountRaw = usdcToRaw(amount)
    const payload = await getAccountWithdrawalPayload(session.accountId, receiver, amountRaw)
    const signature = await signMessage({ message: payload.message, nonce: payload.nonce, recipient: payload.recipient })
    const result = await submitAccountWithdrawal({ ...payload, signature })
    showToast('success', `Withdrawal submitted. Transaction ${result.transaction_hash.slice(0, 8)}…`)
    void refreshDashboard()
  }

  async function authorizeAndSubmit(path: string, body: Record<string, unknown>, signatureNonce: string | number, quoteRecipient?: string) {
    const configuredContract = import.meta.env.VITE_A1_CONTRACT_ID || (import.meta.env.VITE_NEAR_NETWORK === 'mainnet' ? 'a1.afterbell.near' : 'afterbellfx-dev-faucet.testnet')
    const signature = await signMessage(walletPayload(String(body.message), signatureNonce, quoteRecipient || configuredContract))
    return postTrade(path, { ...body, owner_signature: signature })
  }

  async function submitOrder() {
    if (!session || submitting) return
    setSubmitting(true)
    try {
      const notional = Math.round(Number(size) * 1e6)
      const base = { account_id: session.accountId, market_id: market.ticker, side, notional_usdc: String(notional), leverage_bps: Math.round(leverage * 10000), max_slippage_bps: 100 }
      if (orderType === 'limit') {
        if (!Number(limitPrice) || Number(limitPrice) <= 0) throw new Error('Enter a valid limit price.')
        const draft = { account_id: session.accountId, order_id: `o_${crypto.randomUUID().replaceAll('-', '').slice(0, 24)}`, market_id: market.ticker, side, notional_usdc: String(notional), leverage_bps: Math.round(leverage * 10000), limit_price: priceToRaw(Number(limitPrice)), expires_at_ms: Date.now() + 86_400_000 }
        const quote = await postTrade<Record<string, any>>('/v1/trade/limits/quote', draft)
        if (quote.fee_usdc != null) setEstimatedFee(usdc(quote.fee_usdc))
        await authorizeAndSubmit('/v1/trade/limits', { ...draft, ...quote }, quote.nonce)
      } else {
        const quote = await postTrade<Record<string, any>>('/v1/trade/quote', base)
        setEstimatedFee(usdc(quote.fee_usdc))
        await authorizeAndSubmit('/v1/trade/open', quote, quote.nonce)
      }
      showToast('success', `${side === 'long' ? 'Long' : 'Short'} order submitted.`)
      await refreshDashboard()
    } catch (error) { showToast('error', error instanceof Error ? error.message : 'Order could not be submitted.') }
    finally { setSubmitting(false) }
  }

  async function closePosition(position: PositionRecord) {
    if (!session || submitting) return
    setSubmitting(true)
    try {
      const quote = await postTrade<Record<string, any>>('/v1/trade/close/quote', { account_id: session.accountId, position_id: position.position_id, max_slippage_bps: 100 })
      await authorizeAndSubmit('/v1/trade/close', { ...quote, market_id: position.market_id, side: position.is_long ? 'long' : 'short', notional_usdc: position.notional_usdc }, quote.nonce)
      showToast('success', `${position.market_id} position closed.`)
      await refreshDashboard()
    } catch (error) { showToast('error', error instanceof Error ? error.message : 'Position could not be closed.') }
    finally { setSubmitting(false) }
  }

  async function cancelLimit(order: LimitOrderRecord) {
    if (!session || submitting) return
    setSubmitting(true)
    try {
      const quote = await postTrade<Record<string, any>>('/v1/trade/limits/cancel/quote', { account_id: session.accountId, order_id: order.order_id })
      await authorizeAndSubmit('/v1/trade/limits/cancel', { ...quote, account_id: session.accountId, order_id: order.order_id, market_id: order.market_id, side: order.is_long ? 'long' : 'short', notional_usdc: order.notional_usdc }, quote.nonce)
      showToast('success', 'Limit order cancelled.')
      await refreshDashboard()
    } catch (error) { showToast('error', error instanceof Error ? error.message : 'Limit order could not be cancelled.') }
    finally { setSubmitting(false) }
  }

  useEffect(() => {
    if (!notificationsOpen && !riskOpen && !sharePosition && !walletModalOpen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setNotificationsOpen(false); setRiskOpen(false); setSharePosition(null); setShareNotice('') }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [notificationsOpen, riskOpen, sharePosition, walletModalOpen])

  useEffect(() => {
    if (!marketMenu) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (event.target instanceof Node && !marketMenuRef.current?.contains(event.target)) setMarketMenu(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [marketMenu])

  function chooseCustomShareBackground(event: ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0]
    if (!file) return
    const reader = new FileReader()
    reader.onload = () => {
      if (typeof reader.result === 'string') {
        setCustomShareBackground(reader.result)
        setShareBackground('custom-image')
      }
    }
    reader.readAsDataURL(file)
    event.currentTarget.value = ''
  }

  async function downloadTradeCard() {
    if (!sharePosition) return
    const canvas = document.createElement('canvas')
    canvas.width = 1600
    canvas.height = 900
    const context = canvas.getContext('2d')
    if (!context) { setShareNotice('Could not create trade card'); return }

    const backgrounds = ['#102b48', '#452d71', '#155a45', '#72452c']
    const backgroundOption = shareBackgrounds.find(option => option.id === shareBackground)
    const imageSource = shareBackground === 'custom-image' ? customShareBackground : backgroundOption?.image
    if (imageSource) {
      try {
        const image = await loadCanvasImage(imageSource)
        const scale = Math.max(canvas.width / image.width, canvas.height / image.height)
        const width = image.width * scale
        const height = image.height * scale
        context.drawImage(image, (canvas.width - width) / 2, (canvas.height - height) / 2, width, height)
      } catch { context.fillStyle = '#111923'; context.fillRect(0, 0, canvas.width, canvas.height) }
      context.fillStyle = '#080d16a8'
      context.fillRect(0, 0, canvas.width, canvas.height)
    } else {
      context.fillStyle = '#0c121b'
      context.fillRect(0, 0, canvas.width, canvas.height)
      const glow = context.createRadialGradient(1400, 40, 10, 1400, 40, 900)
      glow.addColorStop(0, backgrounds[Number(shareBackground.slice(-1)) - 1] ?? backgrounds[0])
      glow.addColorStop(1, '#0c121b')
      context.fillStyle = glow
      context.fillRect(0, 0, canvas.width, canvas.height)
    }

    try {
      const brandImage = await loadCanvasImage(logo)
      context.drawImage(brandImage, 86, 49, 54, 54)
    } catch { /* Keep the wordmark if the logo asset cannot be loaded. */ }
    context.fillStyle = sharePosition.side === 'Long' ? '#123b30' : '#3b2028'
    context.beginPath(); context.roundRect(1370, 63, 140, 58, 18); context.fill()
    try {
      const tickerSvg = shareCardRef.current?.querySelector<SVGSVGElement>('.share-card-market svg')
      if (tickerSvg) {
        const tickerImage = await loadCanvasSvg(tickerSvg)
        context.fillStyle = '#ffffffed'
        context.beginPath(); context.roundRect(90, 304, 70, 70, 35); context.fill()
        context.drawImage(tickerImage, 108, 322, 34, 34)
      }
    } catch { /* The ticker can still be identified by its label. */ }
    context.shadowColor = '#000b'; context.shadowBlur = 8; context.shadowOffsetY = 2
    context.fillStyle = '#f1f5f9'
    context.font = '600 38px system-ui, sans-serif'
    context.fillText('Afterbell', 153, 91)
    context.fillStyle = sharePosition.side === 'Long' ? '#72e3b4' : '#ffabb3'
    context.font = '600 25px system-ui, sans-serif'
    context.fillText(sharePosition.side, 1400, 101)
    context.fillStyle = '#f4f7fb'
    context.font = '650 76px system-ui, sans-serif'
    context.fillText(`${sharePosition.ticker}-USDC`, 185, 365)
    context.font = '650 78px system-ui, sans-serif'
    const pnlWidth = context.measureText(sharePosition.pnl).width
    context.fillStyle = '#9de8c7'
    context.font = '500 34px system-ui, sans-serif'
    const changeWidth = context.measureText(sharePosition.change).width
    const pnlX = 1510 - pnlWidth - changeWidth - 18
    context.fillStyle = '#54dfa4'
    context.font = '650 78px system-ui, sans-serif'
    context.fillText(sharePosition.pnl, pnlX, 375)
    context.fillStyle = '#9de8c7'
    context.font = '500 34px system-ui, sans-serif'
    context.fillText(sharePosition.change, pnlX + pnlWidth + 18, 371)
    const details = [['Position size', sharePosition.size], ['Leverage', sharePosition.leverage], ['Entry price', sharePosition.entry], ['Mark price', sharePosition.mark]]
    details.forEach(([label, value], index) => {
      const x = index % 2 === 0 ? 90 : 800
      const y = index < 2 ? 545 : 695
      context.fillStyle = '#a2afbe'; context.font = '500 25px system-ui, sans-serif'; context.fillText(label, x, y)
      context.fillStyle = '#f1f5f9'; context.font = '700 34px system-ui, sans-serif'; context.fillText(value, x, y + 48)
    })
    context.fillStyle = '#b5d7fb'; context.font = '500 23px system-ui, sans-serif'; context.fillText('afterbellfx.xyz', 1290, 835)
    context.shadowColor = 'transparent'; context.shadowBlur = 0; context.shadowOffsetY = 0
    const link = document.createElement('a')
    link.download = `${sharePosition.ticker.toLowerCase()}-${sharePosition.side.toLowerCase()}-trade.png`
    link.href = canvas.toDataURL('image/png')
    link.click()
    setShareNotice('Trade card downloaded')
  }

  function startResize(axis: 'x' | 'y', event: ReactPointerEvent<HTMLButtonElement>, initial: number) {
    event.preventDefault()
    dragState.current = { axis, start: axis === 'x' ? event.clientX : event.clientY, initial }
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  function moveResize(event: ReactPointerEvent<HTMLButtonElement>) {
    const drag = dragState.current
    if (!drag) return
    if (drag.axis === 'x') setOrderWidth(Math.min(480, Math.max(290, drag.initial - (event.clientX - drag.start))))
    else setChartHeight(Math.min(720, Math.max(300, drag.initial + (event.clientY - drag.start))))
  }

  function endResize() { dragState.current = null }

  return <main className="trade-app">
    <section className="mobile-trade-wip">
      <a href="/" className="trade-brand"><img className="trade-logo" src={logo} alt=""/><span>Afterbell</span></a>
      <div className="mobile-wip-content"><span className="mobile-wip-icon"><Icon name="chart"/></span><span className="mobile-wip-badge">WIP</span><h1>Trading on mobile is coming.</h1><p>We’re shaping a focused trading experience for smaller screens. For now, open Afterbell on desktop.</p><a className="mobile-wip-back" href="/">← <span>Back to connect wallet</span></a></div>
      <span className="mobile-wip-footer">© 2026 Afterbell</span>
    </section>
    <header className="trade-header">
      <a href="/" className="trade-brand"><img className="trade-logo" src={logo} alt=""/><span>Afterbell</span></a>
      <nav className="trade-nav" aria-label="Main navigation"><a className="nav-active" href="/trade">Trade</a><a href="/markets">Markets</a><a href="/lp">LP</a><a href="#agent">AI agent</a></nav>
      <div className="header-right">
        <button className={`icon-button header-bell ${notificationsOpen ? 'is-open' : ''}`} aria-label="Notifications" aria-expanded={notificationsOpen} onClick={() => setNotificationsOpen(open => !open)}>
          <Icon name="bell" />
          {notifications.length > 0 && <span className="notification-count">{notifications.length}</span>}
        </button>
        <button className="account-balance-widget" aria-label="Open USDC account balance" onClick={() => setBalanceModalOpen(true)}>
          <UsdcIcon size={19}/><span>{availableBalance === null ? <span className="balance-widget-skeleton"/> : money(availableBalance)}</span>
        </button>
        <button className="account-button" title={`${session?.walletName ?? 'Wallet'} · ${chainName}`} onClick={() => setWalletModalOpen(true)}>
          <img className="account-avatar wallet-chain-avatar connected-wallet-icon" src={walletIcon} alt=""/>
          <span className="account-address">{session ? `${session.address.slice(0, 6)}...${session.address.slice(-4)}` : 'Connect wallet'}</span>
          <Icon name="chevron" />
        </button>
      </div>
    </header>

    <div className="trade-main">
      <section className="market-overview">
        <div className="market-selector-wrap" ref={marketMenuRef}>
          <button className="market-selector" aria-expanded={marketMenu} onClick={() => { setMarketMenu(open => !open); setMarketSearch('') }}><span className="stock-avatar"><TickerIcon ticker={market.ticker} size={20}/></span><span className="market-selector-text"><b>{market.ticker}</b></span><Icon name="chevron"/></button>
          {marketMenu && <div className="market-menu"><label className="market-search"><Icon name="search"/><input autoFocus value={marketSearch} onChange={event => setMarketSearch(event.target.value)} placeholder="Search stocks" aria-label="Search stocks"/></label><div className="market-menu-list">{filteredMarkets.map(item => <button key={item.ticker} className={item.ticker === market.ticker ? 'selected' : ''} onClick={() => { setSelectedMarket(item.ticker); setMarketMenu(false); setMarketSearch('') }}><span className="stock-avatar small"><TickerIcon ticker={item.ticker} size={17}/></span><span><b>{item.ticker}</b><small>{item.name} / US equity</small></span></button>)}{filteredMarkets.length === 0 && <p className="market-empty">No stocks found.</p>}</div></div>}
        </div>
        <div className="overview-price"><small className="metric-label" tabIndex={0} aria-describedby="tip-mark-price">Perp mark price<span className="metric-tooltip" id="tip-mark-price" role="tooltip">The current market mark used for position valuation and risk checks.</span></small><strong>{metricsLoadedMarket !== market.ticker ? <span className="metric-skeleton"/> : latestPrice ? money(latestPrice) : 'Unavailable'}</strong><span>{metricsLoadedMarket !== market.ticker ? <span className="metric-skeleton metric-skeleton-small"/> : snapshot?.observed_at_ms ? `Updated ${elapsed(snapshot.observed_at_ms)}` : 'Waiting for oracle price'}</span></div>
        <div className="overview-metric"><small className="metric-label" tabIndex={0} aria-describedby="tip-volume">24h volume<span className="metric-tooltip" id="tip-volume" role="tooltip">Notional value of recorded opens and closes in the last 24 hours.</span></small><strong>{metricsLoadedMarket !== market.ticker ? <span className="metric-skeleton"/> : volumes[market.ticker] ? money(usdc(volumes[market.ticker])) : '$0.00'}</strong></div>
        <div className="overview-metric"><small className="metric-label" tabIndex={0} aria-describedby="tip-open-interest">Open interest<span className="metric-tooltip" id="tip-open-interest" role="tooltip">Combined open long and short notional on A1.</span></small><strong>{metricsLoadedMarket !== market.ticker ? <span className="metric-skeleton"/> : snapshot ? money(usdc(snapshot.open_interest_usdc)) : 'Unavailable'}</strong></div>
        <div className="overview-metric funding"><small className="metric-label" tabIndex={0} aria-describedby="tip-funding">Funding / 8h<span className="metric-tooltip" id="tip-funding" role="tooltip">Current hourly funding rate projected across eight hours.</span></small><strong>{metricsLoadedMarket !== market.ticker ? <span className="metric-skeleton"/> : snapshot ? `${(snapshot.funding_rate_bps_8h / 100).toFixed(4)}%` : 'Unavailable'}</strong></div>
        <div className="stock-quote"><small>{market.ticker}-USDC</small><strong>{metricsLoadedMarket !== market.ticker ? <span className="metric-skeleton"/> : latestPrice ? money(latestPrice) : 'Unavailable'}</strong></div>
      </section>

      <div className="trading-grid" style={{ '--order-width': `${orderWidth}px`, '--chart-height': `${chartHeight}px` } as CSSProperties}>
        <section className="market-workspace" aria-label="Market chart and positions"><PriceChart market={market} points={pricePoints} range={chartRange} onRangeChange={setChartRange}/>
          <button className="resize-handle resize-horizontal" aria-label="Resize chart and positions panels" onPointerDown={event => startResize('y', event, chartHeight)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize}><span/></button>
          <div className="positions-panel">
            <div className="positions-heading"><div className="position-tabs">{['Positions', 'Open orders', 'History'].map(tab => <button key={tab} className={activeTab === tab ? 'active' : ''} onClick={() => setActiveTab(tab)}>{tab}{tab === 'Positions' && <span className="tab-count">{positions.length}</span>}</button>)}</div></div>
            <div className="position-table-wrap"><table className="position-table"><thead><tr><th>Market</th><th>Side</th><th>Size</th><th>{activeTab === 'Open orders' ? 'Limit price' : 'Margin'}</th><th>{activeTab === 'History' ? 'Event' : 'Leverage'}</th><th>{activeTab === 'History' ? 'Time' : 'Entry price'}</th><th>{activeTab === 'History' ? 'Transaction' : 'Mark price'}</th><th>{activeTab === 'History' ? 'Status' : 'Unrealized PnL'}</th><th/></tr></thead><tbody>
              {activeTab === 'Positions' && positions.map(position => {
                const ticker = position.market_id, sideLabel = position.is_long ? 'Long' : 'Short'
                const pnl = usdc(position.unrealized_pnl_usdc), notional = usdc(position.notional_usdc), entry = dollars(position.entry_price), mark = dollars(position.mark_price)
                const margin = usdc(position.margin_usdc ?? String(Math.ceil(Number(position.notional_usdc) * 10000 / position.leverage_bps)))
                const share: SharedPosition = { ticker, side: sideLabel, size: money(notional), margin: money(margin), leverage: `${(position.leverage_bps / 10000).toFixed(1)}×`, entry: money(entry), mark: money(mark), pnl: `${pnl >= 0 ? '+' : ''}${money(pnl)}`, change: `${notional ? (pnl / notional * 100).toFixed(2) : '0.00'}%` }
                return <tr key={position.position_id}><td><span className="table-market"><span className="stock-avatar tiny"><TickerIcon ticker={ticker} size={13}/></span><b>{ticker}</b><small>USDC</small></span></td><td><span className={position.is_long ? 'long-side' : 'short-side'}>{position.is_long ? '↗ Long' : '↘ Short'}</span></td><td>{money(notional)}</td><td>{money(margin)}</td><td>{share.leverage}</td><td>{money(entry)}</td><td>{mark ? money(mark) : 'Unavailable'}</td><td><span className={pnl >= 0 ? 'green-pnl' : 'short-side'}>{share.pnl} <small>({share.change})</small></span></td><td><div className="position-actions"><button className="share-position" aria-label={`Share ${ticker} trade`} onClick={() => { setSharePosition(share); setShareNotice('') }}>Share</button><button className="close-position" disabled={submitting} onClick={() => void closePosition(position)}>Close</button></div></td></tr>
              })}
              {activeTab === 'Open orders' && limitOrders.map(order => <tr key={order.order_id}><td><span className="table-market"><span className="stock-avatar tiny"><TickerIcon ticker={order.market_id} size={13}/></span><b>{order.market_id}</b><small>USDC</small></span></td><td><span className={order.is_long ? 'long-side' : 'short-side'}>{order.is_long ? '↗ Long' : '↘ Short'}</span></td><td>{money(usdc(order.notional_usdc))}</td><td>{money(dollars(order.limit_price))}</td><td>{(order.leverage_bps / 10000).toFixed(1)}×</td><td>Limit</td><td>—</td><td>Pending</td><td><button className="close-position" disabled={submitting} onClick={() => void cancelLimit(order)}>Cancel</button></td></tr>)}
              {activeTab === 'History' && history.map(item => <tr key={item.id}><td><span className="table-market"><span className="stock-avatar tiny">{item.market_id && <TickerIcon ticker={item.market_id} size={13}/>}</span><b>{item.market_id || 'Account'}</b><small>USDC</small></span></td><td>{item.side || '—'}</td><td>{money(usdc(item.amount_usdc))}</td><td>—</td><td>{item.event_type.replaceAll('_', ' ')}</td><td>{elapsed(item.created_at_ms)}</td><td>{item.transaction_hash ? `${item.transaction_hash.slice(0, 8)}…` : '—'}</td><td>Finalized</td><td/></tr>)}
              {((activeTab === 'Positions' && positions.length === 0) || (activeTab === 'Open orders' && limitOrders.length === 0) || (activeTab === 'History' && history.length === 0)) && <tr><td colSpan={9} className="empty-table-state">{activeTab === 'Positions' ? 'No open positions.' : activeTab === 'Open orders' ? 'No open orders.' : 'No account activity yet.'}</td></tr>}
            </tbody></table></div>
            {notice && <div className="trade-notice" role="status">{notice}<button onClick={() => setNotice('')} aria-label="Dismiss">×</button></div>}
          </div>
        </section>
        <button className="resize-handle resize-vertical" aria-label="Resize order panel" onPointerDown={event => startResize('x', event, orderWidth)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize}><span/></button>
        <aside className="order-card" aria-label="Place an order">
          <div className="order-card-heading"><h1>Place order</h1></div>
          <div className="side-switch"><button className={side === 'long' ? 'active long-active' : ''} onClick={() => setSide('long')}><span>↗</span> Long</button><button className={side === 'short' ? 'active short-active' : ''} onClick={() => setSide('short')}><span>↘</span> Short</button></div>
          <div className="order-types"><button className={orderType === 'market' ? 'active' : ''} onClick={() => setOrderType('market')}>Market</button><button className={orderType === 'limit' ? 'active' : ''} onClick={() => setOrderType('limit')}>Limit</button><button className="order-info help-trigger" aria-label="Market and limit order help" aria-describedby="tip-order-types">i<span className="trade-tooltip" id="tip-order-types" role="tooltip">Market orders execute promptly at available prices. Limit orders execute only at your limit price or better, and may not fill.</span></button></div>
          {orderType === 'limit' && <label className="field-label">Limit price<div className="input-wrap"><span>$</span><input inputMode="decimal" value={limitPrice} onChange={event => setLimitPrice(event.target.value)} placeholder="Enter limit price" aria-label="Limit price"/></div></label>}
          <label className="field-label">Order size<div className="input-wrap"><span>$</span><input inputMode="decimal" value={size} onChange={e => { setSize(e.target.value); setPayment((Number(e.target.value || 0) / leverage).toFixed(2)) }} aria-label="Order size"/><span className="input-unit"><UsdcIcon/>USDC</span></div><small className="input-subtext">Notional size</small></label>
          <label className="field-label payment-label"><span>Margin</span><span className="available-label">Available <b>{availableBalance === null ? '—' : money(availableBalance)}</b> <button onClick={() => { if (availableBalance !== null) { setPayment(availableBalance.toFixed(2)); setSize((availableBalance * leverage).toFixed(2)) } }}>Max</button></span><div className="input-wrap"><span>$</span><input inputMode="decimal" value={payment} onChange={e => { setPayment(e.target.value); setSize((Number(e.target.value || 0) * leverage).toFixed(2)) }} aria-label="Margin amount"/><span className="input-unit"><UsdcIcon/>USDC</span></div></label>
          <div className={`leverage-control ${side}`} style={{ '--leverage-progress': `${((leverage - 1) / Math.max(1, maxLeverage - 1)) * 100}%` } as CSSProperties}><div className="leverage-heading"><span>Leverage</span><strong>{leverage.toFixed(1)}×</strong></div><input type="range" min="1" max={maxLeverage} step="0.5" value={Math.min(leverage, maxLeverage)} onChange={e => { const value = Number(e.target.value); setLeverage(value); setSize((Number(payment || 0) * value).toFixed(2)) }} aria-label={`Leverage, maximum ${maxLeverage} times`}/><div className="range-labels">{[1, 3, 5, 7, 10].filter(value => value <= maxLeverage || value === 1).map(value => <span className={leverage >= value ? 'reached' : ''} key={value}>{value}×</span>)}</div></div>
          <div className="order-summary"><div><span>Entry price</span><b>{latestPrice ? money(latestPrice) : 'Unavailable'}</b></div><div><span>Position value</span><b>{money(Number(size || 0))}</b></div><div><span>Liquidation</span><b>Uses A1 maintenance rules</b></div><div><span>Trading fee</span><b>{estimatedFee === null ? 'Calculated in quote' : money(estimatedFee)}</b></div></div>
          <button className={`submit-order ${side}`} disabled={submitting || !tradingEnabled} onClick={() => void submitOrder()}>{submitting ? 'Confirming…' : tradingEnabled ? (side === 'long' ? 'Open long' : 'Open short') : 'Market unavailable'} <span>{money(Number(size || 0))}</span></button>
        </aside>
      </div>
    </div>
    <footer className="trade-statusbar"><InterfaceSpeed/><span className="status-spacer"/><button className="footer-link" onClick={() => setRiskOpen(true)}>Risk disclosure</button><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>

    {notificationsOpen && <><button className="drawer-backdrop" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}/><aside className="notification-drawer" aria-label="Notifications"><div className="drawer-heading"><div><h2>Notifications</h2><p>Recent account activity</p></div><button className="drawer-close" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}>×</button></div><div className="drawer-actions"><span>{notifications.length} {notifications.length === 1 ? 'notification' : 'notifications'}</span><button disabled={notifications.length === 0} onClick={() => session && void clearNotifications(session.accountId).then(() => setNotifications([])).catch(error => showToast('error', error.message))}>Clear all</button></div><div className="notification-list">{notifications.map(item => <article className="notification-item" key={item.id}><div className="notification-body"><div className="notification-title-row"><h3>{item.title}</h3><time>{elapsed(item.created_at_ms)}</time></div><p>{item.message}</p></div><button className="notification-delete" aria-label="Delete notification" title="Delete notification" onClick={() => session && void deleteNotification(session.accountId, item.id).then(() => setNotifications(current => current.filter(notification => notification.id !== item.id))).catch(error => showToast('error', error.message))}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3m3 0-.8 13H6.8L6 7m4 4v5m4-5v5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/></svg></button></article>)}{notifications.length === 0 && <div className="notifications-empty"><span>✓</span><h3>All clear</h3><p>You have no notifications.</p></div>}</div></aside></>}

    {walletModalOpen && session && <WalletDetailsModal session={session} onClose={() => setWalletModalOpen(false)} onCopy={copyWalletAddress} onDisconnect={disconnectWallet} />}
    {balanceModalOpen && session && <AccountBalanceModal session={session} balanceRaw={balanceRaw} nearBalanceRaw={nearWalletBalanceRaw} deposit={depositInstructions} onClose={() => setBalanceModalOpen(false)} onCopy={copyWalletAddress} onWithdraw={withdrawFromAccount}/>}
    {riskOpen && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) setRiskOpen(false) }}><section className="risk-modal" role="dialog" aria-modal="true" aria-labelledby="risk-title"><header className="risk-modal-header"><div><span className="risk-eyebrow">PLEASE READ BEFORE TRADING</span><h2 id="risk-title">Risk disclosure</h2><p>Understand the product and its risks before you trade.</p></div><button className="drawer-close" aria-label="Close risk disclosure" onClick={() => setRiskOpen(false)}>×</button></header><div className="risk-modal-content">{riskDisclosureSections.map(section => <section key={section.title}><h3>{section.title}</h3><p>{section.body}</p></section>)}</div><footer className="risk-modal-footer"><a href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Read the product documentation.</a><button onClick={() => setRiskOpen(false)}>I understand</button></footer></section></div>}
    {sharePosition && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) { setSharePosition(null); setShareNotice('') } }}><section className="share-modal" role="dialog" aria-modal="true" aria-labelledby="share-title"><header className="risk-modal-header"><div><h2 id="share-title">Share your trade</h2><p>A snapshot of this position, ready to share.</p></div><button className="drawer-close" aria-label="Close share trade dialog" onClick={() => { setSharePosition(null); setShareNotice('') }}>×</button></header><div className="share-modal-content"><article ref={shareCardRef} className={`share-trade-card ${shareBackground}`} style={shareBackground.startsWith('image-') || shareBackground === 'custom-image' ? { backgroundImage: `linear-gradient(90deg, #0b111ad9, #0b111a9c), url(${shareBackground === 'custom-image' ? customShareBackground : shareBackgrounds.find(option => option.id === shareBackground)?.image})` } : undefined}><div className="share-card-brand"><img src={logo} alt=""/><span>Afterbell</span><span className={`share-card-side ${sharePosition.side.toLowerCase()}`}>{sharePosition.side}</span></div><div className="share-card-main"><div className="share-card-market"><span className="stock-avatar"><TickerIcon ticker={sharePosition.ticker} size={20}/></span><div><b>{sharePosition.ticker}-USDC</b></div></div><div className="share-card-profit"><strong>{sharePosition.pnl}</strong><span>{sharePosition.change}</span></div></div><div className="share-card-details"><div><span>Position size</span><b>{sharePosition.size}</b></div><div><span>Leverage</span><b>{sharePosition.leverage}</b></div><div><span>Entry price</span><b>{sharePosition.entry}</b></div><div><span>Mark price</span><b>{sharePosition.mark}</b></div></div><div className="share-card-footer">afterbellfx.xyz</div></article><div className="share-background-picker"><div className="share-background-heading">Background</div><div className="share-background-options">{shareBackgrounds.map(option => <button key={option.id} className={`share-background-option ${option.id} ${shareBackground === option.id ? 'selected' : ''}`} aria-label={option.label} aria-pressed={shareBackground === option.id} title={option.label} style={option.kind === 'image' ? { backgroundImage: `linear-gradient(#080c1280, #080c1280), url(${option.image})` } : undefined} onClick={() => setShareBackground(option.id)} />)}<label className={`share-background-option custom-image ${shareBackground === 'custom-image' ? 'selected' : ''}`} title="Upload custom image" aria-label="Upload custom image" style={customShareBackground ? { backgroundImage: `linear-gradient(#080c1280, #080c1280), url(${customShareBackground})` } : undefined}><input type="file" accept="image/*" onChange={chooseCustomShareBackground}/><span>+</span></label></div></div>{shareNotice && <p className="share-notice" role="status">{shareNotice}</p>}</div><footer className="share-modal-footer"><button className="share-action-button" onClick={() => void downloadTradeCard()}>Download</button></footer></section></div>}
  </main>
}
