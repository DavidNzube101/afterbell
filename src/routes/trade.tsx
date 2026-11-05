import { createFileRoute } from '@tanstack/react-router'
import { useState, useRef, useEffect, type CSSProperties, type PointerEvent as ReactPointerEvent, type ChangeEvent } from 'react'
import logo from '../assets/afterbell-logo.png'
import shareImage1 from '../assets/share-images/1.png'
import shareImage2 from '../assets/share-images/2.png'
import shareImage3 from '../assets/share-images/3.png'
import { TickerIcon } from '../components/TickerIcon'
import { markets } from '../data/markets'
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

function UsdcIcon() {
  return <svg className="usdc-icon" role="img" aria-label="USDC" viewBox="0 0 32 32"><circle cx="16" cy="16" r="16" fill="#2775CA"/><path d="M20.2 18.4c0-2.2-1.3-3-4-3.3-1.9-.3-2.3-.8-2.3-1.7s.7-1.5 2-1.5c1.2 0 1.8.4 2.1 1.4.1.2.2.3.4.3h1c.3 0 .4-.2.4-.4-.3-1.4-1.4-2.5-2.9-2.6V9c0-.2-.2-.4-.5-.4h-.9c-.3 0-.4.2-.5.4v1.5c-1.9.3-3.1 1.5-3.1 3.1 0 2.1 1.3 2.9 4 3.2 1.8.3 2.4.7 2.4 1.7 0 1-.9 1.7-2.1 1.7-1.7 0-2.2-.7-2.4-1.6-.1-.3-.3-.4-.5-.4h-1c-.3 0-.4.2-.4.4.3 1.6 1.2 2.6 3.2 2.9V23c0 .2.2.4.5.4h.9c.3 0 .4-.2.5-.4v-1.5c1.9-.3 3.2-1.6 3.2-3.1z" fill="#fff"/><path d="M12.8 24.9c-4.9-1.8-7.4-7.2-5.6-12 .9-2.6 3-4.6 5.6-5.6.3-.1.4-.3.4-.6v-.9c0-.2-.1-.4-.4-.5h-.2C6.8 7.2 3.5 13.1 5.3 19c1.1 3.5 3.8 6.1 7.3 7.3.3.1.5 0 .6-.3v-1.1c0-.1-.2-.3-.4-.4zm6.5-19.6c-.3-.1-.5 0-.6.3v.9c0 .3.1.5.4.6 4.9 1.8 7.4 7.2 5.6 12-.9 2.6-3 4.6-5.6 5.6-.3.1-.4.3-.4.6v.9c0 .2.1.4.4.5h.2c5.9-1.9 9.2-8.2 7.3-14.1-1.1-3.6-3.9-6.2-7.3-7.3z" fill="#fff"/></svg>
}

function loadCanvasImage(source: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = reject
    image.src = source
  })
}

let singleTickerWidget: Promise<void> | undefined

function loadSingleTickerWidget() {
  if (customElements.get('tv-single-ticker')) return Promise.resolve()
  if (!singleTickerWidget) {
    singleTickerWidget = new Promise<void>((resolve, reject) => {
      const script = document.createElement('script')
      script.type = 'module'
      script.src = 'https://widgets.tradingview-widget.com/w/en/tv-single-ticker.js'
      script.onload = () => { void customElements.whenDefined('tv-single-ticker').then(() => resolve()) }
      script.onerror = () => reject(new Error('TradingView ticker failed to load'))
      document.head.appendChild(script)
    })
  }
  return singleTickerWidget
}

function StockQuote({ ticker }: { ticker: string }) {
  const quoteRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const container = quoteRef.current
    if (!container) return
    let active = true
    container.replaceChildren()
    const placeholder = document.createElement('span')
    placeholder.className = 'stock-quote-placeholder'
    placeholder.textContent = 'Loading price'
    container.appendChild(placeholder)

    void loadSingleTickerWidget().then(() => {
      if (!active || !quoteRef.current) return
      const widget = document.createElement('tv-single-ticker')
      widget.setAttribute('symbol', `NASDAQ:${ticker}`)
      widget.setAttribute('theme', 'dark')
      widget.setAttribute('hide-market-status', '')
      container.replaceChildren(widget)
    }).catch(() => {
      if (active) placeholder.textContent = 'Price unavailable'
    })

    return () => {
      active = false
      container.replaceChildren()
    }
  }, [ticker])

  return <div className="stock-quote"><small>Stock price</small><div className="stock-quote-widget" ref={quoteRef} aria-label={`${ticker} stock price`} /></div>
}

let advancedChartWidget: Promise<void> | undefined

function loadAdvancedChartWidget() {
  if (customElements.get('tv-advanced-chart')) return Promise.resolve()
  if (!advancedChartWidget) {
    advancedChartWidget = new Promise<void>((resolve, reject) => {
      const script = document.createElement('script')
      script.type = 'module'
      script.src = 'https://widgets.tradingview-widget.com/w/en/tv-advanced-chart.js'
      script.onload = () => { void customElements.whenDefined('tv-advanced-chart').then(() => resolve()) }
      script.onerror = () => reject(new Error('TradingView chart failed to load'))
      document.head.appendChild(script)
    })
  }
  return advancedChartWidget
}

function PriceChart({ market }: { market: (typeof markets)[number] }) {
  const symbol = `NASDAQ:${market.ticker}`
  const chartRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const container = chartRef.current
    if (!container) return
    container.replaceChildren()

    let active = true
    const placeholder = document.createElement('span')
    placeholder.className = 'chart-placeholder'
    placeholder.textContent = 'Loading chart'
    container.appendChild(placeholder)

    void loadAdvancedChartWidget().then(() => {
      if (!active) return
      const chart = document.createElement('tv-advanced-chart')
      chart.setAttribute('symbol', symbol)
      chart.setAttribute('interval', '15')
      chart.setAttribute('timezone', 'Etc/UTC')
      chart.setAttribute('chart-type', 'candles')
      chart.setAttribute('theme', 'dark')
      chart.style.width = '100%'
      chart.style.height = '100%'
      container.replaceChildren(chart)
    }).catch(() => {
      if (active) placeholder.textContent = 'Chart unavailable. Check your connection and reload.'
    })
    return () => { active = false; container.replaceChildren() }
  }, [symbol])

  return <div className="chart-shell">
    <div className="chart-tools"><div className="chart-pair"><span>{market.ticker} / USD</span></div><span className="chart-source">TradingView</span></div>
    <div ref={chartRef} className="real-market-chart" aria-label={`${market.name} reference equity chart`} />
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

const initialNotifications = [
  { id: 1, title: 'Margin level changed', message: 'Your NVDA position is approaching its maintenance margin.', time: 'Just now', type: 'risk' },
  { id: 2, title: 'Agent activity', message: 'Your trading agent opened a position on META.', time: '12 min ago', type: 'agent' },
  { id: 3, title: 'Withdrawal completed', message: 'Your USDC withdrawal has been delivered to Base.', time: 'Yesterday', type: 'success' },
]

const riskDisclosureSections = [
  { title: 'Perpetual contracts are not stocks', body: 'Afterbell contracts provide synthetic, cash-settled price exposure to referenced US equities. They do not give you shares, ownership, voting rights, dividends, corporate-action entitlements, or a claim against NVIDIA, Tesla, Apple, Alphabet, Meta, or any other issuer. Contract prices can differ from the prices of the referenced equity, especially outside regular market hours.' },
  { title: 'Leverage, margin and liquidation', body: 'Leverage magnifies both gains and losses. A small move in the reference price can materially reduce your margin. Positions may be liquidated automatically when their equity falls below required maintenance levels; a liquidation may occur before you can respond, and execution may be worse than expected. You can lose all collateral assigned to a position. Fees, funding payments and price impact also affect results.' },
  { title: 'Pricing and execution', body: 'Prices depend on external market data, oracle publication, signed execution quotes and the operation of keepers. Data can be delayed, unavailable, inaccurate or manipulated. Oracle or quote-signer compromise or error may result in incorrect prices or execution. Freshness checks, deviation limits and circuit breakers reduce some risks but cannot eliminate them. Trading may pause or execute at prices that differ from a displayed reference price. After-hours equity and token prices may be thin, volatile or disconnected from regular-session prices.' },
  { title: 'Shared liquidity vault', body: 'The vault is the counterparty to positions. Liquidity providers bear trading, utilization, oracle, liquidation, bad-debt and smart-contract risks. Vault shares are not deposits, are not guaranteed to maintain a fixed value, and withdrawals may be delayed or limited by protocol rules. Losses can reduce vault assets and the value of LP shares.' },
  { title: 'Wallets, smart contracts and networks', body: 'You authorize actions with your connected wallet. You are responsible for reviewing each signature, transaction, destination and agent permission. Blockchain transactions may be irreversible. Contract defects, compromised keys, network outages, congestion, forks, wallet failures or third-party service failures can result in delayed, failed or lost transactions or assets. Protocol administration, configuration changes and operational services also create risk; code limits and time delays are safeguards, not guarantees against errors or compromise.' },
  { title: 'Stablecoins, cross-chain funding and agents', body: 'USDC and other supported digital assets may lose value, become unavailable, or be affected by issuer, reserve, bridge, token-contract or redemption risk. Cross-chain deposits and withdrawals depend on NEAR Intents and external networks; they may be delayed, fail or be refunded under their applicable processes. An AI agent can act only within its on-chain permissions, but its decisions may still lose money. Monitor its activity and revoke its permissions when you no longer want it to trade.' },
  { title: 'No guarantees; access restrictions', body: 'Trading involves substantial risk and may not be suitable for you. Do not trade with funds you cannot afford to lose. Past performance does not predict future results, and no strategy, agent or protocol feature guarantees profit or prevents loss. Use of Afterbell is subject to applicable laws and geographic restrictions; do not access the service where it is prohibited.' },
]

type SharedPosition = { ticker: string; side: 'Long' | 'Short'; size: string; margin: string; leverage: string; entry: string; mark: string; pnl: string; change: string }

const mockPositions: SharedPosition[] = [
  { ticker: 'NVDA', side: 'Long', size: '$1,240.00', margin: '$413.33', leverage: '3.0×', entry: '$139.42', mark: '$142.87', pnl: '+$30.69', change: '+7.43%' },
  { ticker: 'META', side: 'Short', size: '$760.00', margin: '$380.00', leverage: '2.0×', entry: '$719.10', mark: '$712.66', pnl: '+$6.81', change: '+1.79%' },
]

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
  const { market: selectedTicker, side: initialSide } = Route.useSearch()
  const [marketIndex, setMarketIndex] = useState(() => Math.max(0, markets.findIndex(item => item.ticker === selectedTicker)))
  const [side, setSide] = useState<'long' | 'short'>(initialSide ?? 'long')
  const [orderType, setOrderType] = useState<'market' | 'limit'>('market')
  const [leverage, setLeverage] = useState(3)
  const [size, setSize] = useState('250')
  const [payment, setPayment] = useState('100')
  const [activeTab, setActiveTab] = useState('Positions')
  const [marketMenu, setMarketMenu] = useState(false)
  const [marketSearch, setMarketSearch] = useState('')
  const [notificationsOpen, setNotificationsOpen] = useState(false)
  const [notifications, setNotifications] = useState(initialNotifications)
  const [riskOpen, setRiskOpen] = useState(false)
  const [sharePosition, setSharePosition] = useState<SharedPosition | null>(null)
  const [shareBackground, setShareBackground] = useState('gradient-1')
  const [customShareBackground, setCustomShareBackground] = useState<string | null>(null)
  const [shareNotice, setShareNotice] = useState('')
  const [notice, setNotice] = useState('')
  const [orderWidth, setOrderWidth] = useState(342)
  const [chartHeight, setChartHeight] = useState(485)
  const marketMenuRef = useRef<HTMLDivElement>(null)
  const dragState = useRef<{ axis: 'x' | 'y'; start: number; initial: number } | null>(null)
  const market = markets[marketIndex]
  const filteredMarkets = markets.filter(item => `${item.ticker} ${item.name}`.toLowerCase().includes(marketSearch.trim().toLowerCase()))
  const positionValue = Number(payment || 0) * leverage

  useEffect(() => {
    if (!notificationsOpen && !riskOpen && !sharePosition) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setNotificationsOpen(false); setRiskOpen(false); setSharePosition(null); setShareNotice('') }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [notificationsOpen, riskOpen, sharePosition])

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

    context.fillStyle = '#f1f5f9'
    context.font = '600 38px system-ui, sans-serif'
    context.fillText('Afterbell', 90, 112)
    context.fillStyle = sharePosition.side === 'Long' ? '#123b30' : '#3b2028'
    context.beginPath(); context.roundRect(1370, 63, 140, 58, 18); context.fill()
    context.fillStyle = sharePosition.side === 'Long' ? '#72e3b4' : '#ffabb3'
    context.font = '600 25px system-ui, sans-serif'
    context.fillText(sharePosition.side, 1400, 101)
    context.fillStyle = '#f4f7fb'
    context.font = '650 76px system-ui, sans-serif'
    context.fillText(sharePosition.ticker, 90, 375)
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
      <nav className="trade-nav" aria-label="Main navigation"><a className="nav-active" href="/trade">Trade</a><a href="/markets">Markets</a><a href="#agent">AI agent</a></nav>
      <div className="header-right"><button className={`icon-button header-bell ${notificationsOpen ? 'is-open' : ''}`} aria-label="Notifications" aria-expanded={notificationsOpen} onClick={() => setNotificationsOpen(open => !open)}><Icon name="bell"/>{notifications.length > 0 && <span className="notification-count">{notifications.length}</span>}</button><button className="account-button" onClick={() => setNotice('Wallet connection will be added in the next integration step.')}><span className="account-avatar">D</span><span className="account-address">0x84...b29a</span><Icon name="chevron"/></button></div>
    </header>

    <div className="trade-main">
      <section className="market-overview">
        <div className="market-selector-wrap" ref={marketMenuRef}>
          <button className="market-selector" aria-expanded={marketMenu} onClick={() => { setMarketMenu(open => !open); setMarketSearch('') }}><span className="stock-avatar"><TickerIcon ticker={market.ticker} size={20}/></span><span className="market-selector-text"><b>{market.ticker}</b></span><Icon name="chevron"/></button>
          {marketMenu && <div className="market-menu"><label className="market-search"><Icon name="search"/><input autoFocus value={marketSearch} onChange={event => setMarketSearch(event.target.value)} placeholder="Search stocks" aria-label="Search stocks"/></label><div className="market-menu-list">{filteredMarkets.map((item) => { const i = markets.indexOf(item); return <button key={item.ticker} className={i === marketIndex ? 'selected' : ''} onClick={() => { setMarketIndex(i); setMarketMenu(false); setMarketSearch('') }}><span className="stock-avatar small"><TickerIcon ticker={item.ticker} size={17}/></span><span><b>{item.ticker}</b><small>{item.name} / US equity</small></span></button> })}{filteredMarkets.length === 0 && <p className="market-empty">No stocks found.</p>}</div></div>}
        </div>
        <div className="overview-price"><small className="metric-label" tabIndex={0} aria-describedby="tip-mark-price">Perp mark price<span className="metric-tooltip" id="tip-mark-price" role="tooltip">The reference price used to value open positions and help determine liquidations. It can differ from the price available to trade.</span></small><strong>Unavailable</strong><span>Awaiting market feed</span></div>
        <div className="overview-metric"><small className="metric-label" tabIndex={0} aria-describedby="tip-volume">24h volume<span className="metric-tooltip" id="tip-volume" role="tooltip">The total value of trades recorded in this market over the past 24 hours.</span></small><strong>Unavailable</strong></div>
        <div className="overview-metric"><small className="metric-label" tabIndex={0} aria-describedby="tip-open-interest">Open interest<span className="metric-tooltip" id="tip-open-interest" role="tooltip">The current total value of open positions in this market.</span></small><strong>Unavailable</strong></div>
        <div className="overview-metric funding"><small className="metric-label" tabIndex={0} aria-describedby="tip-funding">Funding / 8h<span className="metric-tooltip" id="tip-funding" role="tooltip">The funding rate applied for each 8-hour interval. Funding payments help keep the contract price aligned with its reference market.</span></small><strong>Unavailable</strong></div>
        <StockQuote ticker={market.ticker}/>
      </section>

      <div className="trading-grid" style={{ '--order-width': `${orderWidth}px`, '--chart-height': `${chartHeight}px` } as CSSProperties}>
        <section className="market-workspace" aria-label="Market chart and positions"><PriceChart market={market}/>
          <button className="resize-handle resize-horizontal" aria-label="Resize chart and positions panels" onPointerDown={event => startResize('y', event, chartHeight)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize}><span/></button>
          <div className="positions-panel">
            <div className="positions-heading"><div className="position-tabs">{['Positions', 'Open orders', 'History'].map(tab => <button key={tab} className={activeTab === tab ? 'active' : ''} onClick={() => setActiveTab(tab)}>{tab}{tab === 'Positions' && <span className="tab-count">2</span>}</button>)}</div></div>
            <div className="position-table-wrap"><table className="position-table"><thead><tr><th>Market</th><th>Side</th><th>Size</th><th>Margin</th><th>Leverage</th><th>Entry price</th><th>Mark price</th><th>Unrealized PnL</th><th/></tr></thead><tbody>
              {mockPositions.map(position => <tr key={position.ticker}><td><span className="table-market"><span className="stock-avatar tiny"><TickerIcon ticker={position.ticker} size={13}/></span><b>{position.ticker}</b><small>US equity</small></span></td><td><span className={position.side === 'Long' ? 'long-side' : 'short-side'}>{position.side === 'Long' ? '↗' : '↘'} {position.side}</span></td><td>{position.size}</td><td>{position.margin}</td><td>{position.leverage}</td><td>{position.entry}</td><td>{position.mark}</td><td><span className="green-pnl">{position.pnl} <small>({position.change})</small></span></td><td><div className="position-actions"><button className="share-position" aria-label={`Share ${position.ticker} trade`} onClick={() => { setSharePosition(position); setShareNotice('') }}>Share</button><button className="close-position" onClick={() => setNotice('Position closing will be available when trading is connected.')}>Close</button></div></td></tr>)}
            </tbody></table></div>
            {notice && <div className="trade-notice" role="status">{notice}<button onClick={() => setNotice('')} aria-label="Dismiss">×</button></div>}
          </div>
        </section>
        <button className="resize-handle resize-vertical" aria-label="Resize order panel" onPointerDown={event => startResize('x', event, orderWidth)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize}><span/></button>
        <aside className="order-card" aria-label="Place an order">
          <div className="order-card-heading"><h1>Place order</h1></div>
          <div className="side-switch"><button className={side === 'long' ? 'active long-active' : ''} onClick={() => setSide('long')}><span>↗</span> Long</button><button className={side === 'short' ? 'active short-active' : ''} onClick={() => setSide('short')}><span>↘</span> Short</button></div>
          <div className="order-types"><button className={orderType === 'market' ? 'active' : ''} onClick={() => setOrderType('market')}>Market</button><button className={orderType === 'limit' ? 'active' : ''} onClick={() => setOrderType('limit')}>Limit</button><button className="order-info help-trigger" aria-label="Market and limit order help" aria-describedby="tip-order-types">i<span className="trade-tooltip" id="tip-order-types" role="tooltip">Market orders execute promptly at available prices. Limit orders execute only at your limit price or better, and may not fill.</span></button></div>
          {orderType === 'limit' && <label className="field-label">Limit price<div className="input-wrap"><span>$</span><input inputMode="decimal" placeholder="Enter limit price" aria-label="Limit price"/></div></label>}
          <label className="field-label">Order size<div className="input-wrap"><span>$</span><input inputMode="decimal" value={size} onChange={e => setSize(e.target.value)} aria-label="Order size"/><span className="input-unit"><UsdcIcon/>USDC</span></div><small className="input-subtext">Notional size</small></label>
          <label className="field-label payment-label"><span>Margin</span><span className="available-label">Available <b>$1,240.00</b> <button onClick={() => setPayment('1240')}>Max</button></span><div className="input-wrap"><span>$</span><input inputMode="decimal" value={payment} onChange={e => setPayment(e.target.value)} aria-label="Margin amount"/><span className="input-unit"><UsdcIcon/>USDC</span></div></label>
          <div className={`leverage-control ${side}`} style={{ '--leverage-progress': `${((leverage - 1) / 9) * 100}%` } as CSSProperties}><div className="leverage-heading"><span>Leverage</span><strong>{leverage.toFixed(1)}×</strong></div><input type="range" min="1" max="10" step="0.5" value={leverage} onChange={e => setLeverage(Number(e.target.value))} aria-label="Leverage"/><div className="range-labels">{[1, 3, 5, 7, 10].map(value => <span className={leverage >= value ? 'reached' : ''} key={value}>{value}×</span>)}</div></div>
          <label className="tp-sl-toggle"><input type="checkbox"/><span className="custom-check"/><span>Take profit / Stop loss</span><button className="help-trigger" type="button" aria-label="Take profit and stop loss help" aria-describedby="tip-tp-sl">i<span className="trade-tooltip" id="tip-tp-sl" role="tooltip">Set trigger prices to close a position automatically when reached. The final execution price may differ from your trigger.</span></button></label>
          <div className="order-summary"><div><span>Entry price</span><b>See chart</b></div><div><span>Position value</span><b>${positionValue.toFixed(2)}</b></div><div><span>Liquidation price</span><b>Unavailable</b></div><div><span>Trading fee</span><b>Unavailable</b></div></div>
          <button className={`submit-order ${side}`} onClick={() => setNotice('Order preview only. Connect trading to submit.')}>{side === 'long' ? 'Open long' : 'Open short'} <span>${positionValue.toFixed(2)}</span></button>
          <p className="order-disclaimer">Orders are simulated in this preview.</p>
        </aside>
      </div>
    </div>
    <footer className="trade-statusbar"><InterfaceSpeed/><span className="status-spacer"/><button className="footer-link" onClick={() => setRiskOpen(true)}>Risk disclosure</button><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>

    {notificationsOpen && <><button className="drawer-backdrop" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}/><aside className="notification-drawer" aria-label="Notifications"><div className="drawer-heading"><div><h2>Notifications</h2><p>Recent account activity</p></div><button className="drawer-close" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}>×</button></div><div className="drawer-actions"><span>{notifications.length} {notifications.length === 1 ? 'notification' : 'notifications'}</span><button disabled={notifications.length === 0} onClick={() => setNotifications([])}>Clear all</button></div><div className="notification-list">{notifications.map(item => <article className="notification-item" key={item.id}><div className="notification-body"><div className="notification-title-row"><h3>{item.title}</h3><time>{item.time}</time></div><p>{item.message}</p></div><button className="notification-delete" aria-label="Delete notification" title="Delete notification" onClick={() => setNotifications(current => current.filter(notification => notification.id !== item.id))}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3m3 0-.8 13H6.8L6 7m4 4v5m4-5v5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/></svg></button></article>)}{notifications.length === 0 && <div className="notifications-empty"><span>✓</span><h3>All clear</h3><p>You have no notifications.</p></div>}</div></aside></>}

    {riskOpen && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) setRiskOpen(false) }}><section className="risk-modal" role="dialog" aria-modal="true" aria-labelledby="risk-title"><header className="risk-modal-header"><div><span className="risk-eyebrow">PLEASE READ BEFORE TRADING</span><h2 id="risk-title">Risk disclosure</h2><p>Understand the product and its risks before you trade.</p></div><button className="drawer-close" aria-label="Close risk disclosure" onClick={() => setRiskOpen(false)}>×</button></header><div className="risk-modal-content">{riskDisclosureSections.map(section => <section key={section.title}><h3>{section.title}</h3><p>{section.body}</p></section>)}</div><footer className="risk-modal-footer"><span>Questions? Visit our documentation.</span><button onClick={() => setRiskOpen(false)}>I understand</button></footer></section></div>}
    {sharePosition && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) { setSharePosition(null); setShareNotice('') } }}><section className="share-modal" role="dialog" aria-modal="true" aria-labelledby="share-title"><header className="risk-modal-header"><div><h2 id="share-title">Share your trade</h2><p>A snapshot of this position, ready to share.</p></div><button className="drawer-close" aria-label="Close share trade dialog" onClick={() => { setSharePosition(null); setShareNotice('') }}>×</button></header><div className="share-modal-content"><article className={`share-trade-card ${shareBackground}`} style={shareBackground.startsWith('image-') || shareBackground === 'custom-image' ? { backgroundImage: `linear-gradient(90deg, #0b111ad9, #0b111a9c), url(${shareBackground === 'custom-image' ? customShareBackground : shareBackgrounds.find(option => option.id === shareBackground)?.image})` } : undefined}><div className="share-card-brand"><img src={logo} alt=""/><span>Afterbell</span><span className={`share-card-side ${sharePosition.side.toLowerCase()}`}>{sharePosition.side}</span></div><div className="share-card-main"><div className="share-card-market"><span className="stock-avatar"><TickerIcon ticker={sharePosition.ticker} size={20}/></span><div><b>{sharePosition.ticker}</b></div></div><div className="share-card-profit"><strong>{sharePosition.pnl}</strong><span>{sharePosition.change}</span></div></div><div className="share-card-details"><div><span>Position size</span><b>{sharePosition.size}</b></div><div><span>Leverage</span><b>{sharePosition.leverage}</b></div><div><span>Entry price</span><b>{sharePosition.entry}</b></div><div><span>Mark price</span><b>{sharePosition.mark}</b></div></div><div className="share-card-footer">afterbellfx.xyz</div></article><div className="share-background-picker"><div className="share-background-heading">Background</div><div className="share-background-options">{shareBackgrounds.map(option => <button key={option.id} className={`share-background-option ${option.id} ${shareBackground === option.id ? 'selected' : ''}`} aria-label={option.label} aria-pressed={shareBackground === option.id} title={option.label} style={option.kind === 'image' ? { backgroundImage: `linear-gradient(#080c1280, #080c1280), url(${option.image})` } : undefined} onClick={() => setShareBackground(option.id)} />)}<label className={`share-background-option custom-image ${shareBackground === 'custom-image' ? 'selected' : ''}`} title="Upload custom image" aria-label="Upload custom image" style={customShareBackground ? { backgroundImage: `linear-gradient(#080c1280, #080c1280), url(${customShareBackground})` } : undefined}><input type="file" accept="image/*" onChange={chooseCustomShareBackground}/><span>+</span></label></div></div>{shareNotice && <p className="share-notice" role="status">{shareNotice}</p>}</div><footer className="share-modal-footer"><button className="share-action-button" onClick={() => void downloadTradeCard()}>Download</button></footer></section></div>}
  </main>
}
