import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useState, useRef, useEffect, useMemo, type CSSProperties, type PointerEvent as ReactPointerEvent, type ChangeEvent } from 'react'
import { AccountBalanceModal } from '../components/AccountBalanceModal'
import { TradeBootstrapScreen } from '../components/TradeBootstrapScreen'
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
import { clearNotifications, clearTradeBootstrapData, deleteNotification, dollars, getAccountBalance, getAccountWithdrawalPayload, getDepositInstructions, getHistory, getLimits, getMarketHistory, getMarketVolumes, getNearWalletBalance, getNotifications, getPositions, getSnapshots, getTradeBootstrapData, postTrade, preloadTradeDashboard, priceToRaw, submitAccountWithdrawal, usdc, usdcToRaw, walletPayload, type ActivityRecord, type LimitOrderRecord, type MarketSnapshot, type NotificationRecord, type PositionRecord, type PricePoint, type VolumePoint } from '../lib/dashboard'
import { useToasts } from '../lib/toasts'
import './trade.css'

export const Route = createFileRoute('/trade')({
  validateSearch: (search: Record<string, unknown>): { market: string | undefined; side: 'long' | 'short' | undefined } => ({
    market: typeof search.market === 'string' ? search.market : undefined,
    side: search.side === 'long' || search.side === 'short' ? search.side : undefined,
  }),
  component: TradeEntry,
})

function TradeEntry() {
  const navigate = useNavigate()
  const { session } = useWalletSession()
  const { market: selectedMarket } = Route.useSearch()
  const marketId = selectedMarket ?? markets[0].ticker
  const [attempt, setAttempt] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const [ready, setReady] = useState(() => Boolean(session && getTradeBootstrapData(session.accountId, marketId, '1h')))

  useEffect(() => {
    if (!session) {
      void navigate({ to: '/', replace: true })
      return
    }
    let active = true
    const cached = getTradeBootstrapData(session.accountId, marketId, '1h')
    if (cached) {
      setReady(true)
      return () => { active = false }
    }
    setReady(false)
    setError(null)
    void preloadTradeDashboard(session.accountId, marketId).then(() => {
      if (active) setReady(true)
    }).catch(loadError => {
      if (active) setError(loadError instanceof Error ? loadError.message : 'Could not load the trading workspace.')
    })
    return () => { active = false }
  }, [session?.accountId, marketId, attempt, navigate])

  if (!session || !ready) return <TradeBootstrapScreen error={error} onRetry={() => setAttempt(value => value + 1)} />
  return <TradingPage />
}

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
const candleIntervalMs: Record<ChartRange, number> = { '1h': 900_000, '1d': 3_600_000, '1w': 14_400_000, '1m': 86_400_000, all: 86_400_000 }
function PriceChart({ market, points, volumePoints, range, onRangeChange }: { market: Market; points: PricePoint[]; volumePoints: VolumePoint[]; range: ChartRange; onRangeChange: (range: ChartRange) => void }) {
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const hoverRef = useRef<{ index: number; y: number } | null>(null)
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null)
  const candles = useMemo(() => {
    const buckets = new Map<number, { open: number; high: number; low: number; close: number; volume: number }>()
    const interval = candleIntervalMs[range]
    const volumes = new Map<number, number>()
    for (const point of volumePoints) {
      const bucket = Math.floor(point.timestamp_ms / interval) * interval
      const amount = Number(BigInt(point.volume_usdg_raw)) / 1_000_000
      if (Number.isFinite(amount)) volumes.set(bucket, (volumes.get(bucket) ?? 0) + amount)
    }
    for (const point of [...points].sort((left, right) => left.timestamp_ms - right.timestamp_ms)) {
      const value = dollars(point.price_e8)
      if (!Number.isFinite(value) || value <= 0) continue
      const bucket = Math.floor(point.timestamp_ms / interval) * interval
      const current = buckets.get(bucket)
      if (current) { current.high = Math.max(current.high, value); current.low = Math.min(current.low, value); current.close = value }
      else buckets.set(bucket, { open: value, high: value, low: value, close: value, volume: 0 })
    }
    return Array.from(buckets, ([timestamp, candle]) => ({ time: timestamp, ...candle, volume: volumes.get(timestamp) ?? 0 }))
      .sort((left, right) => left.time - right.time)
  }, [points, volumePoints, range])

  useEffect(() => {
    const canvas = canvasRef.current
    const context = canvas?.getContext('2d')
    if (!canvas || !context) return
    hoverRef.current = null
    let paintFrame = 0
    const paint = () => {
      const width = canvas.clientWidth
      const height = canvas.clientHeight
      if (!width || !height) return
      const dpr = window.devicePixelRatio || 1
      const backingWidth = Math.round(width * dpr)
      const backingHeight = Math.round(height * dpr)
      if (canvas.width !== backingWidth || canvas.height !== backingHeight) {
        canvas.width = backingWidth
        canvas.height = backingHeight
      }
      context.setTransform(dpr, 0, 0, dpr, 0, 0)
      context.clearRect(0, 0, width, height)
      if (!candles.length) return

      const left = 12
      const right = Math.min(width * 0.2, 76)
      const top = 20
      const bottom = 26
      const volumeHeight = Math.min(74, Math.max(42, height * 0.18))
      const volumeGap = 12
      const priceBottom = height - bottom - volumeHeight - volumeGap
      const plotWidth = width - left - right
      const plotHeight = priceBottom - top
      if (plotWidth <= 0 || plotHeight <= 0) return

      const minPrice = Math.min(...candles.map(candle => candle.low))
      const maxPrice = Math.max(...candles.map(candle => candle.high))
      const padding = Math.max((maxPrice - minPrice) * 0.1, maxPrice * 0.001)
      const low = Math.max(0, minPrice - padding)
      const high = maxPrice + padding
      const timeStart = candles[0].time - candleIntervalMs[range] / 2
      const timeEnd = candles[candles.length - 1].time + candleIntervalMs[range] / 2
      const timeSpan = Math.max(candleIntervalMs[range], timeEnd - timeStart)
      const x = (time: number) => left + (time - timeStart) / timeSpan * plotWidth
      const y = (price: number) => top + (high - price) / (high - low || 1) * plotHeight
      const font = getComputedStyle(canvas).fontFamily
      context.font = `12px ${font}`
      context.fillStyle = '#18212d'
      context.beginPath()
      for (let gx = left; gx <= width - right; gx += 24) {
        for (let gy = top; gy <= height - bottom; gy += 24) {
          context.moveTo(gx + 0.7, gy)
          context.arc(gx, gy, 0.7, 0, Math.PI * 2)
        }
      }
      context.fill()
      context.fillStyle = '#8492a3'
      context.textAlign = 'left'
      for (let tick = 0; tick <= 4; tick++) {
        const value = high - (high - low) * tick / 4
        context.fillText(value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 }), width - right + 8, top + plotHeight * tick / 4 + 4)
      }

      const maxVolume = Math.max(1, ...candles.map(candle => candle.volume))
      const bodyWidth = Math.max(1, Math.min(12, plotWidth * candleIntervalMs[range] / timeSpan * 0.72))
      for (const candle of candles) {
        const centerX = x(candle.time)
        const openY = y(candle.open)
        const closeY = y(candle.close)
        const color = candle.close >= candle.open ? '#18c77a' : '#ff583f'
        context.strokeStyle = color
        context.lineWidth = 1
        context.beginPath()
        context.moveTo(centerX, y(candle.high))
        context.lineTo(centerX, y(candle.low))
        context.stroke()
        context.fillStyle = color
        context.fillRect(centerX - bodyWidth / 2, Math.min(openY, closeY), bodyWidth, Math.max(1.5, Math.abs(closeY - openY)))
        if (candle.volume > 0) {
          const barHeight = candle.volume / maxVolume * (volumeHeight - 4)
          context.globalAlpha = 0.55
          context.fillRect(centerX - bodyWidth / 2, height - bottom - barHeight, bodyWidth, barHeight)
          context.globalAlpha = 1
        }
      }

      const last = candles[candles.length - 1]
      const lastY = y(last.close)
      context.strokeStyle = 'rgba(132, 146, 163, .42)'
      context.setLineDash([4, 5])
      context.beginPath()
      context.moveTo(left, lastY)
      context.lineTo(width - right, lastY)
      context.stroke()
      context.setLineDash([])
      context.fillStyle = '#10161f'
      context.fillRect(width - right + 2, lastY - 12, right - 4, 24)
      context.strokeStyle = last.close >= last.open ? '#18c77a' : '#ff583f'
      context.strokeRect(width - right + 2, lastY - 12, right - 4, 24)
      context.fillStyle = last.close >= last.open ? '#18c77a' : '#ff583f'
      context.fillText(last.close.toFixed(2), width - right + 8, lastY + 4)

      context.fillStyle = '#8492a3'
      const timeTicks = width < 480 ? 3 : 5
      for (let tick = 0; tick < timeTicks; tick++) {
        const time = timeStart + timeSpan * tick / (timeTicks - 1)
        const options: Intl.DateTimeFormatOptions = range === '1w' || range === '1m' || range === 'all'
          ? { month: 'short', day: 'numeric' }
          : { hour: '2-digit', minute: '2-digit' }
        context.textAlign = tick === 0 ? 'left' : tick === timeTicks - 1 ? 'right' : 'center'
        context.fillText(new Date(time).toLocaleString([], options), x(time), height - 6)
      }
      context.textAlign = 'left'
      if (hoverRef.current) {
        const candle = candles[hoverRef.current.index]
        const crossX = x(candle.time)
        context.strokeStyle = 'rgba(132, 146, 163, .55)'
        context.lineWidth = 0.7
        context.beginPath()
        context.moveTo(crossX, top)
        context.lineTo(crossX, height - bottom)
        context.moveTo(left, hoverRef.current.y)
        context.lineTo(width - right, hoverRef.current.y)
        context.stroke()
      }
    }
    const schedulePaint = () => {
      if (paintFrame) return
      paintFrame = window.requestAnimationFrame(() => {
        paintFrame = 0
        paint()
      })
    }
    const onPointerMove = (event: PointerEvent) => {
      if (!candles.length) return
      const rect = canvas.getBoundingClientRect()
      const px = event.clientX - rect.left
      const start = candles[0].time - candleIntervalMs[range] / 2
      const end = candles[candles.length - 1].time + candleIntervalMs[range] / 2
      const right = Math.min(canvas.clientWidth * 0.2, 76)
      const t = start + (px - 12) / (canvas.clientWidth - 12 - right) * (end - start)
      const index = candles.reduce((best, candle, i) => Math.abs(candle.time - t) < Math.abs(candles[best].time - t) ? i : best, 0)
      hoverRef.current = { index, y: event.clientY - rect.top }
      setHoveredIndex(current => current === index ? current : index)
      schedulePaint()
    }
    const onPointerLeave = () => { hoverRef.current = null; setHoveredIndex(null); schedulePaint() }
    canvas.addEventListener('pointermove', onPointerMove)
    canvas.addEventListener('pointerleave', onPointerLeave)
    const resize = new ResizeObserver(schedulePaint)
    resize.observe(canvas.parentElement ?? canvas)
    paint()
    return () => {
      resize.disconnect()
      if (paintFrame) window.cancelAnimationFrame(paintFrame)
      canvas.removeEventListener('pointermove', onPointerMove)
      canvas.removeEventListener('pointerleave', onPointerLeave)
    }
  }, [candles, range])

  const latest = candles[candles.length - 1]
  const displayCandle = hoveredIndex === null ? latest : candles[hoveredIndex] ?? latest
  return <div className="chart-shell">
    <div className="chart-tools">
      <div className="chart-pair"><span>{market.ticker} / USDC</span></div>
      <div className="chart-actions" role="group" aria-label="Chart time range">{chartRanges.map(item => <button key={item} className={`chart-range-button ${range === item ? 'active' : ''}`} onClick={() => onRangeChange(item)}>{item.toUpperCase()}</button>)}</div>
    </div>
    <div className="real-market-chart" role="img" aria-label={`${market.name} candlestick chart`}>
      {candles.length === 0 ? <div className="chart-placeholder">
        {latest && <strong>${latest.close.toFixed(2)}</strong>}
        <span>{latest ? 'Collecting more A1 price updates to build the chart.' : 'Waiting for fresh market price data.'}</span>
      </div> : <>
        <canvas ref={canvasRef} className="oracle-candlestick-canvas" aria-label={`${market.ticker} / USDC OHLC price history`} />
        {displayCandle && <div className="chart-ohlc" aria-label="Selected candle"><span>O <b>{displayCandle.open.toFixed(2)}</b></span><span>H <b>{displayCandle.high.toFixed(2)}</b></span><span>L <b>{displayCandle.low.toFixed(2)}</b></span><span>C <b>{displayCandle.close.toFixed(2)}</b></span></div>}
      </>}
    </div>
  </div>
}

type PerformanceState = { ms: number | null }

function InterfaceSpeed() {
  const [speed, setSpeed] = useState<PerformanceState>({ ms: null })

  useEffect(() => {
    let active = true
    const measureNetwork = async () => {
      const controller = new AbortController()
      const timeout = window.setTimeout(() => controller.abort(), 5000)
      const startedAt = performance.now()
      try {
        await fetch(`/api/health/live?probe=${Date.now()}`, {
          method: 'GET',
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
  const initialMarket = selectedTicker ?? markets[0].ticker
  const bootstrap = session ? getTradeBootstrapData(session.accountId, initialMarket, '1h') : null
  const [selectedMarket, setSelectedMarket] = useState(initialMarket)
  const { markets: availableMarkets } = useMarketRegistry()
  const [side, setSide] = useState<'long' | 'short'>(initialSide ?? 'long')
  const [orderType, setOrderType] = useState<'market' | 'limit'>('market')
  const [leverage, setLeverage] = useState(3)
  const [size, setSize] = useState('250')
  const [payment, setPayment] = useState('100')
  const orderSizeInputRef = useRef<HTMLInputElement>(null)
  const marginInputRef = useRef<HTMLInputElement>(null)
  const limitPriceInputRef = useRef<HTMLInputElement>(null)
  const limitPriceDraftRef = useRef('')
  const lastEditedOrderFieldRef = useRef<'size' | 'margin'>('size')
  const [activeTab, setActiveTab] = useState('Positions')
  const [marketMenu, setMarketMenu] = useState(false)
  const [marketSearch, setMarketSearch] = useState('')
  const [notificationsOpen, setNotificationsOpen] = useState(false)
  const [notifications, setNotifications] = useState<NotificationRecord[]>([])
  const [positions, setPositions] = useState<PositionRecord[]>(() => bootstrap?.positions ?? [])
  const [limitOrders, setLimitOrders] = useState<LimitOrderRecord[]>([])
  const [history, setHistory] = useState<ActivityRecord[]>([])
  const [pricePoints, setPricePoints] = useState<PricePoint[]>(() => bootstrap?.points ?? [])
  const [volumePoints, setVolumePoints] = useState<VolumePoint[]>(() => bootstrap?.volumePoints ?? [])
  const [historyStartMs, setHistoryStartMs] = useState<number | null>(() => bootstrap?.historyStartMs ?? null)
  const [snapshots, setSnapshots] = useState<MarketSnapshot[]>(() => bootstrap?.snapshots ?? [])
  const [volumes, setVolumes] = useState<Record<string, string>>(() => bootstrap?.volumes ?? {})
  const [chartRange, setChartRange] = useState<ChartRange>('1h')
  const [metricsLoadedMarket, setMetricsLoadedMarket] = useState<string | null>(() => bootstrap?.marketId ?? null)
  const [availableBalance, setAvailableBalance] = useState<number | null>(() => bootstrap?.balanceRaw != null ? usdc(bootstrap.balanceRaw) : null)
  const [balanceRaw, setBalanceRaw] = useState<string | null>(() => bootstrap?.balanceRaw ?? null)
  const [nearWalletBalanceRaw, setNearWalletBalanceRaw] = useState<string | null>(null)
  const [depositInstructions, setDepositInstructions] = useState<{ contract_id: string; token_id: string; network: string } | null>(() => bootstrap?.depositInstructions ?? null)
  const [balanceModalOpen, setBalanceModalOpen] = useState(false)
  const [walletModalOpen, setWalletModalOpen] = useState(false)
  const [estimatedFee, setEstimatedFee] = useState<number | null>(null)
  const [submitting, setSubmitting] = useState(false)
  const [riskOpen, setRiskOpen] = useState(false)
  const [sharePosition, setSharePosition] = useState<SharedPosition | null>(null)
  const [shareBackground, setShareBackground] = useState('gradient-1')
  const [customShareBackground, setCustomShareBackground] = useState<string | null>(null)
  const shareCardRef = useRef<HTMLElement>(null)
  const metricsRequestMarketRef = useRef<string | null>(bootstrap?.marketId ?? null)
  const dashboardRefreshInFlightRef = useRef(false)
  const dashboardRefreshPendingRef = useRef(false)
  const skipBootstrappedRefreshRef = useRef(Boolean(bootstrap))
  const refreshDashboardRef = useRef<() => Promise<void>>(async () => undefined)
  const notificationsLoadedRef = useRef(false)
  const notificationsAccountRef = useRef<string | null>(null)
  const activeTabRef = useRef(activeTab)
  const notificationsOpenRef = useRef(notificationsOpen)
  const depositInstructionsRef = useRef(depositInstructions)
  const depositInstructionsAccountRef = useRef<string | null>(bootstrap ? session?.accountId ?? null : null)
  activeTabRef.current = activeTab
  notificationsOpenRef.current = notificationsOpen
  depositInstructionsRef.current = depositInstructions
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
    const skipReactSelectionChangeForTradeInputs = (event: Event) => {
      const active = document.activeElement
      if (active === orderSizeInputRef.current || active === marginInputRef.current || active === limitPriceInputRef.current) {
        event.stopImmediatePropagation()
      }
    }
    window.addEventListener('selectionchange', skipReactSelectionChangeForTradeInputs, true)
    return () => window.removeEventListener('selectionchange', skipReactSelectionChangeForTradeInputs, true)
  }, [])

  useEffect(() => {
    if (snapshot?.max_leverage_bps && leverage > maxLeverage) {
      setLeverage(maxLeverage)
      const nextSize = (Number(marginInputRef.current?.value || payment || 0) * maxLeverage).toFixed(2)
      setSize(nextSize)
      if (orderSizeInputRef.current) orderSizeInputRef.current.value = nextSize
    }
  }, [snapshot?.max_leverage_bps, maxLeverage])

  async function refreshDashboard(skipSnapshot = false) {
    if (!session || document.visibilityState === 'hidden') return
    if (dashboardRefreshInFlightRef.current) {
      dashboardRefreshPendingRef.current = true
      return
    }
    dashboardRefreshInFlightRef.current = true
    const requestedMarket = market.ticker
    const requestedRange = chartRange
    if (metricsRequestMarketRef.current !== market.ticker) {
      metricsRequestMarketRef.current = market.ticker
      setMetricsLoadedMarket(null)
    }
    const tasks: Promise<unknown>[] = [
      ...(!skipSnapshot ? [getSnapshots().then(data => setSnapshots(Array.isArray(data) ? data : []))] : []),
      getMarketVolumes().then(rows => { setVolumes(Object.fromEntries(rows.map(row => [row.market_id, row.volume_usdc_raw]))) }),
      getMarketHistory(requestedMarket, requestedRange).then(data => {
        if (metricsRequestMarketRef.current !== requestedMarket) return
        setPricePoints(Array.isArray(data.points) ? data.points : [])
        setVolumePoints(Array.isArray(data.volume_points) ? data.volume_points : [])
        setHistoryStartMs(data.history_start_ms)
      }),
      getPositions(session.accountId).then(data => setPositions(data.positions)),
      ...(activeTabRef.current === 'Open orders' ? [getLimits(session.accountId).then(data => setLimitOrders(data.orders))] : []),
      ...(activeTabRef.current === 'History' ? [getHistory(session.accountId).then(data => setHistory(data.events))] : []),
      ...(!notificationsLoadedRef.current || notificationsAccountRef.current !== session.accountId || notificationsOpenRef.current ? [getNotifications(session.accountId).then(data => {
        setNotifications(data.notifications)
        notificationsLoadedRef.current = true
        notificationsAccountRef.current = session.accountId
      })] : []),
      getAccountBalance(session.accountId).then(data => {
        const raw = String(data.balance_usdc)
        setBalanceRaw(raw)
        setAvailableBalance(usdc(raw))
      }),
      ...(!depositInstructionsRef.current || depositInstructionsAccountRef.current !== session.accountId ? [getDepositInstructions(session.accountId).then(data => {
        setDepositInstructions(data)
        depositInstructionsAccountRef.current = session.accountId
      })] : []),
    ]
    try {
      await Promise.allSettled(tasks)
      if (metricsRequestMarketRef.current === requestedMarket) setMetricsLoadedMarket(requestedMarket)
    } finally {
      dashboardRefreshInFlightRef.current = false
      if (dashboardRefreshPendingRef.current) {
        dashboardRefreshPendingRef.current = false
        window.setTimeout(() => void refreshDashboardRef.current(), 0)
      }
    }
  }
  refreshDashboardRef.current = refreshDashboard
  useEffect(() => {
    if (!session || balanceModalOpen) return
    if (skipBootstrappedRefreshRef.current) {
      skipBootstrappedRefreshRef.current = false
      clearTradeBootstrapData(session.accountId)
      void refreshDashboard(true)
    } else {
      void refreshDashboard()
    }
    const pollWhenVisible = () => { if (document.visibilityState === 'visible') void refreshDashboard() }
    const timer = window.setInterval(pollWhenVisible, 30000)
    document.addEventListener('visibilitychange', pollWhenVisible)
    return () => { window.clearInterval(timer); document.removeEventListener('visibilitychange', pollWhenVisible) }
  }, [session?.accountId, market.ticker, chartRange, balanceModalOpen, activeTab, notificationsOpen])

  useEffect(() => {
    if (!balanceModalOpen || session?.chain !== 'near') return
    let active = true
    void getNearWalletBalance(session.address).then(data => {
      if (active) setNearWalletBalanceRaw(data.amount_yocto)
    }).catch(() => undefined)
    return () => { active = false }
  }, [balanceModalOpen, session?.chain, session?.address])

  function selectChartRange(next: ChartRange) {
    const requiredMs: Record<ChartRange, number> = {
      '1h': 60 * 60 * 1000,
      '1d': 24 * 60 * 60 * 1000,
      '1w': 7 * 24 * 60 * 60 * 1000,
      '1m': 30 * 24 * 60 * 60 * 1000,
      all: 30 * 24 * 60 * 60 * 1000,
    }
    const coverageMs = historyStartMs === null ? 0 : Date.now() - historyStartMs
    if (coverageMs < requiredMs[next]) {
      showToast('waiting', 'We’re still collecting enough market history for that chart range.')
      return
    }
    setChartRange(next)
  }

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
      const enteredMargin = marginInputRef.current?.value.trim() ?? payment
      const enteredSize = lastEditedOrderFieldRef.current === 'margin'
        ? (Number(enteredMargin) * leverage).toFixed(2)
        : (orderSizeInputRef.current?.value ?? size).trim()
      const enteredLimitPrice = limitPriceInputRef.current?.value ?? limitPriceDraftRef.current
      const sourceValue = lastEditedOrderFieldRef.current === 'margin' ? enteredMargin : enteredSize
      if (!/^\d+(\.\d{1,6})?$/.test(sourceValue) || Number(sourceValue) <= 0) {
        throw new Error(lastEditedOrderFieldRef.current === 'margin' ? 'Enter a valid margin greater than zero.' : 'Enter a valid order size greater than zero.')
      }
      const enteredPayment = lastEditedOrderFieldRef.current === 'margin' ? enteredMargin : (Number(enteredSize) / leverage).toFixed(2)
      setSize(enteredSize)
      setPayment(enteredPayment)
      if (orderSizeInputRef.current) orderSizeInputRef.current.value = enteredSize
      if (marginInputRef.current) marginInputRef.current.value = enteredPayment
      const notional = Math.round(Number(enteredSize) * 1e6)
      const base = { account_id: session.accountId, market_id: market.ticker, side, notional_usdc: String(notional), leverage_bps: Math.round(leverage * 10000), max_slippage_bps: 100 }
      if (orderType === 'limit') {
        if (!Number(enteredLimitPrice) || Number(enteredLimitPrice) <= 0) throw new Error('Enter a valid limit price.')
        const draft = { account_id: session.accountId, order_id: `o_${crypto.randomUUID().replaceAll('-', '').slice(0, 24)}`, market_id: market.ticker, side, notional_usdc: String(notional), leverage_bps: Math.round(leverage * 10000), limit_price: priceToRaw(Number(enteredLimitPrice)), expires_at_ms: Date.now() + 86_400_000 }
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
        <section className="market-workspace" aria-label="Market chart and positions"><PriceChart market={market} points={pricePoints} volumePoints={volumePoints} range={chartRange} onRangeChange={selectChartRange}/>
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
          {orderType === 'limit' && <label className="field-label">Limit price<div className="input-wrap"><span>$</span><input ref={limitPriceInputRef} inputMode="decimal" autoComplete="off" defaultValue={limitPriceDraftRef.current} onBlur={event => { limitPriceDraftRef.current = event.currentTarget.value }} placeholder="Enter limit price" aria-label="Limit price"/></div></label>}
          <label className="field-label">Order size<div className="input-wrap"><span>$</span><input ref={orderSizeInputRef} inputMode="decimal" autoComplete="off" maxLength={24} defaultValue={size} onInput={() => { lastEditedOrderFieldRef.current = 'size' }} aria-label="Order size"/><span className="input-unit"><UsdcIcon/>USDC</span></div><small className="input-subtext">Notional size (margin × leverage)</small></label>
          <label className="field-label payment-label"><span>Margin</span><span className="available-label">Available <b>{availableBalance === null ? '—' : money(availableBalance)}</b> <button type="button" onClick={() => { if (availableBalance !== null) { const nextPayment = availableBalance.toFixed(2); const nextSize = (availableBalance * leverage).toFixed(2); lastEditedOrderFieldRef.current = 'margin'; setPayment(nextPayment); setSize(nextSize); if (marginInputRef.current) marginInputRef.current.value = nextPayment; if (orderSizeInputRef.current) orderSizeInputRef.current.value = nextSize } }}>Max</button></span><div className="input-wrap"><span>$</span><input ref={marginInputRef} inputMode="decimal" autoComplete="off" maxLength={24} defaultValue={payment} onInput={() => { lastEditedOrderFieldRef.current = 'margin' }} aria-label="Margin amount"/><span className="input-unit"><UsdcIcon/>USDC</span></div></label>
          <div className={`leverage-control ${side}`} style={{ '--leverage-progress': `${((leverage - 1) / Math.max(1, maxLeverage - 1)) * 100}%` } as CSSProperties}><div className="leverage-heading"><span>Leverage</span><strong>{leverage.toFixed(1)}×</strong></div><input type="range" min="1" max={maxLeverage} step="0.5" value={Math.min(leverage, maxLeverage)} onChange={e => { const value = Number(e.target.value); const nextPayment = marginInputRef.current?.value ?? payment; if (!/^\d+(\.\d{1,6})?$/.test(nextPayment) || Number(nextPayment) <= 0) { setLeverage(value); return } const nextSize = (Number(nextPayment) * value).toFixed(2); lastEditedOrderFieldRef.current = 'margin'; setLeverage(value); setPayment(nextPayment); setSize(nextSize); if (orderSizeInputRef.current) orderSizeInputRef.current.value = nextSize }} aria-label={`Leverage, maximum ${maxLeverage} times`}/><div className="range-labels">{[1, 3, 5, 7, 10].filter(value => value <= maxLeverage || value === 1).map(value => <span className={leverage >= value ? 'reached' : ''} key={value}>{value}×</span>)}</div></div>
          <div className="order-summary"><div><span>Entry price</span><b>{latestPrice ? money(latestPrice) : 'Unavailable'}</b></div><div><span>Position value</span><b>{money(Number(size || 0))}</b></div><div><span>Liquidation</span><b>Uses A1 maintenance rules</b></div><div><span>Trading fee</span><b>{estimatedFee === null ? 'Calculated in quote' : money(estimatedFee)}</b></div></div>
          <button className={`submit-order ${side}`} disabled={submitting || !tradingEnabled} onClick={() => void submitOrder()}>{submitting ? 'Confirming…' : tradingEnabled ? (side === 'long' ? 'Open long' : 'Open short') : 'Market unavailable'} <span>{money(Number(size || 0))}</span></button>
        </aside>
      </div>
    </div>
    <footer className="trade-statusbar"><InterfaceSpeed/><span className="status-spacer"/><button className="footer-link" onClick={() => setRiskOpen(true)}>Risk disclosure</button><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>

    {notificationsOpen && <><button className="drawer-backdrop" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}/><aside className="notification-drawer" aria-label="Notifications"><div className="drawer-heading"><div><h2>Notifications</h2><p>Recent account activity</p></div><button className="drawer-close" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}>×</button></div><div className="drawer-actions"><span>{notifications.length} {notifications.length === 1 ? 'notification' : 'notifications'}</span><button disabled={notifications.length === 0} onClick={() => session && void clearNotifications(session.accountId).then(() => setNotifications([])).catch(error => showToast('error', error.message))}>Clear all</button></div><div className="notification-list">{notifications.map(item => <article className="notification-item" key={item.id}><div className="notification-body"><div className="notification-title-row"><h3>{item.title}</h3><time>{elapsed(item.created_at_ms)}</time></div><p>{item.message}</p></div><button className="notification-delete" aria-label="Delete notification" title="Delete notification" onClick={() => session && void deleteNotification(session.accountId, item.id).then(() => setNotifications(current => current.filter(notification => notification.id !== item.id))).catch(error => showToast('error', error.message))}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3m3 0-.8 13H6.8L6 7m4 4v5m4-5v5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/></svg></button></article>)}{notifications.length === 0 && <div className="notifications-empty"><span>✓</span><h3>All clear</h3><p>You have no notifications.</p></div>}</div></aside></>}

    {walletModalOpen && session && <WalletDetailsModal session={session} onClose={() => setWalletModalOpen(false)} onCopy={copyWalletAddress} onDisconnect={disconnectWallet} />}
    {balanceModalOpen && session && <AccountBalanceModal session={session} balanceRaw={balanceRaw} nearBalanceRaw={nearWalletBalanceRaw} deposit={depositInstructions} onClose={() => setBalanceModalOpen(false)} onCopy={copyWalletAddress} onWithdraw={withdrawFromAccount} onDepositComplete={refreshDashboard}/>}
    {riskOpen && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) setRiskOpen(false) }}><section className="risk-modal" role="dialog" aria-modal="true" aria-labelledby="risk-title"><header className="risk-modal-header"><div><span className="risk-eyebrow">PLEASE READ BEFORE TRADING</span><h2 id="risk-title">Risk disclosure</h2><p>Understand the product and its risks before you trade.</p></div><button className="drawer-close" aria-label="Close risk disclosure" onClick={() => setRiskOpen(false)}>×</button></header><div className="risk-modal-content">{riskDisclosureSections.map(section => <section key={section.title}><h3>{section.title}</h3><p>{section.body}</p></section>)}</div><footer className="risk-modal-footer"><a href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Read the product documentation.</a><button onClick={() => setRiskOpen(false)}>I understand</button></footer></section></div>}
    {sharePosition && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) { setSharePosition(null); setShareNotice('') } }}><section className="share-modal" role="dialog" aria-modal="true" aria-labelledby="share-title"><header className="risk-modal-header"><div><h2 id="share-title">Share your trade</h2><p>A snapshot of this position, ready to share.</p></div><button className="drawer-close" aria-label="Close share trade dialog" onClick={() => { setSharePosition(null); setShareNotice('') }}>×</button></header><div className="share-modal-content"><article ref={shareCardRef} className={`share-trade-card ${shareBackground}`} style={shareBackground.startsWith('image-') || shareBackground === 'custom-image' ? { backgroundImage: `linear-gradient(90deg, #0b111ad9, #0b111a9c), url(${shareBackground === 'custom-image' ? customShareBackground : shareBackgrounds.find(option => option.id === shareBackground)?.image})` } : undefined}><div className="share-card-brand"><img src={logo} alt=""/><span>Afterbell</span><span className={`share-card-side ${sharePosition.side.toLowerCase()}`}>{sharePosition.side}</span></div><div className="share-card-main"><div className="share-card-market"><span className="stock-avatar"><TickerIcon ticker={sharePosition.ticker} size={20}/></span><div><b>{sharePosition.ticker}-USDC</b></div></div><div className="share-card-profit"><strong>{sharePosition.pnl}</strong><span>{sharePosition.change}</span></div></div><div className="share-card-details"><div><span>Position size</span><b>{sharePosition.size}</b></div><div><span>Leverage</span><b>{sharePosition.leverage}</b></div><div><span>Entry price</span><b>{sharePosition.entry}</b></div><div><span>Mark price</span><b>{sharePosition.mark}</b></div></div><div className="share-card-footer">afterbellfx.xyz</div></article><div className="share-background-picker"><div className="share-background-heading">Background</div><div className="share-background-options">{shareBackgrounds.map(option => <button key={option.id} className={`share-background-option ${option.id} ${shareBackground === option.id ? 'selected' : ''}`} aria-label={option.label} aria-pressed={shareBackground === option.id} title={option.label} style={option.kind === 'image' ? { backgroundImage: `linear-gradient(#080c1280, #080c1280), url(${option.image})` } : undefined} onClick={() => setShareBackground(option.id)} />)}<label className={`share-background-option custom-image ${shareBackground === 'custom-image' ? 'selected' : ''}`} title="Upload custom image" aria-label="Upload custom image" style={customShareBackground ? { backgroundImage: `linear-gradient(#080c1280, #080c1280), url(${customShareBackground})` } : undefined}><input type="file" accept="image/*" onChange={chooseCustomShareBackground}/><span>+</span></label></div></div>{shareNotice && <p className="share-notice" role="status">{shareNotice}</p>}</div><footer className="share-modal-footer"><button className="share-action-button" onClick={() => void downloadTradeCard()}>Download</button></footer></section></div>}
  </main>
}
