import { createFileRoute } from '@tanstack/react-router'
import { useState, useRef, useEffect, type CSSProperties, type PointerEvent as ReactPointerEvent } from 'react'
import logo from '../assets/afterbell-logo.png'
import { TickerIcon } from '../components/TickerIcon'
import './trade.css'

export const Route = createFileRoute('/trade')({ component: TradingPage })

const markets = [
  { ticker: 'NVDA', name: 'NVIDIA' },
  { ticker: 'TSLA', name: 'Tesla' },
  { ticker: 'AAPL', name: 'Apple' },
  { ticker: 'GOOGL', name: 'Alphabet' },
  { ticker: 'META', name: 'Meta' },
]

function Icon({ name }: { name: 'chart' | 'chevron' | 'bell' | 'search' }) {
  const paths = {
    chart: <><path d="M4 19V5M4 19h17"/><path d="m7 15 4-5 3 2 6-7"/></>,
    chevron: <path d="m7 10 5 5 5-5"/>,
    bell: <><path d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"/><path d="M10 21h4"/></>,
    search: <><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></>,
  }
  return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>
}

function PriceChart({ market }: { market: (typeof markets)[number] }) {
  const symbol = `NASDAQ:${market.ticker}`
  const chartRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const container = chartRef.current
    if (!container) return
    container.replaceChildren()

    const widget = document.createElement('div')
    widget.className = 'tradingview-widget-container__widget'
    widget.style.height = '100%'
    widget.style.width = '100%'

    const script = document.createElement('script')
    script.type = 'text/javascript'
    script.src = 'https://s3.tradingview.com/external-embedding/embed-widget-advanced-chart.js'
    script.async = true
    script.textContent = JSON.stringify({
      autosize: true,
      symbol,
      interval: '15',
      timezone: 'Etc/UTC',
      theme: 'dark',
      style: '1',
      locale: 'en',
      allow_symbol_change: false,
      calendar: false,
      details: false,
      hide_side_toolbar: false,
      hide_top_toolbar: false,
      hide_legend: false,
      hide_volume: false,
      hotlist: false,
      save_image: false,
      withdateranges: true,
      backgroundColor: '#0c1016',
      gridColor: 'rgba(31, 42, 54, 0.55)',
      support_host: 'https://www.tradingview.com',
    })
    container.append(widget, script)
    return () => container.replaceChildren()
  }, [symbol])

  return <div className="chart-shell">
    <div className="chart-tools"><div className="chart-pair"><span>{market.ticker} / USD</span><span className="chart-pair-muted">Equity reference</span></div><span className="chart-source">TradingView</span></div>
    <div ref={chartRef} className="real-market-chart" aria-label={`${market.name} reference equity chart`} />
  </div>
}

type PerformanceState = { fps: number | null; ms: number | null }

function InterfaceSpeed() {
  const [speed, setSpeed] = useState<PerformanceState>({ fps: null, ms: null })

  useEffect(() => {
    let frame = 0
    let startedAt: number | null = null
    let frameCount = 0
    const measure = (timestamp: number) => {
      if (document.visibilityState === 'hidden') {
        startedAt = null
        frameCount = 0
        frame = requestAnimationFrame(measure)
        return
      }
      if (startedAt === null) startedAt = timestamp
      frameCount += 1
      const elapsed = timestamp - startedAt
      if (elapsed >= 1000 && frameCount > 0) {
        setSpeed(current => ({ ...current, fps: Math.round(frameCount * 1000 / elapsed) }))
        startedAt = timestamp
        frameCount = 0
      }
      frame = requestAnimationFrame(measure)
    }
    frame = requestAnimationFrame(measure)
    return () => cancelAnimationFrame(frame)
  }, [])

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
        if (active) setSpeed(current => ({ ...current, ms: performance.now() - startedAt }))
      } catch {
        if (active) setSpeed(current => ({ ...current, ms: null }))
      } finally {
        window.clearTimeout(timeout)
      }
    }
    void measureNetwork()
    const interval = window.setInterval(() => void measureNetwork(), 10000)
    return () => {
      active = false
      window.clearInterval(interval)
    }
  }, [])

  const grade = speed.ms === null ? 'measuring' : speed.ms <= 100 ? 'good' : speed.ms <= 250 ? 'medium' : 'poor'
  const fpsReadout = speed.fps !== null ? `${speed.fps} FPS` : 'FPS measuring'
  const networkReadout = speed.ms !== null ? `${Math.round(speed.ms)} ms` : 'Latency unavailable'
  return <span className={`interface-speed ${grade}`} title="FPS measures display smoothness. Milliseconds measure the round-trip time to Afterbell API."><span>{fpsReadout}</span><span>{networkReadout}</span></span>
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

function TradingPage() {
  const [marketIndex, setMarketIndex] = useState(0)
  const [side, setSide] = useState<'long' | 'short'>('long')
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
  const [notice, setNotice] = useState('')
  const [orderWidth, setOrderWidth] = useState(342)
  const [chartHeight, setChartHeight] = useState(485)
  const dragState = useRef<{ axis: 'x' | 'y'; start: number; initial: number } | null>(null)
  const market = markets[marketIndex]
  const filteredMarkets = markets.filter(item => `${item.ticker} ${item.name}`.toLowerCase().includes(marketSearch.trim().toLowerCase()))
  const positionValue = Number(payment || 0) * leverage

  useEffect(() => {
    if (!notificationsOpen && !riskOpen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setNotificationsOpen(false); setRiskOpen(false) }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [notificationsOpen, riskOpen])

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
      <nav className="trade-nav" aria-label="Main navigation"><a className="nav-active" href="/trade">Trade</a><a href="#markets">Markets</a><a href="#agent">AI agent</a></nav>
      <div className="header-right"><button className={`icon-button header-bell ${notificationsOpen ? 'is-open' : ''}`} aria-label="Notifications" aria-expanded={notificationsOpen} onClick={() => setNotificationsOpen(open => !open)}><Icon name="bell"/>{notifications.length > 0 && <span className="notification-count">{notifications.length}</span>}</button><button className="account-button" onClick={() => setNotice('Wallet connection will be added in the next integration step.')}><span className="account-avatar">D</span><span className="account-address">0x84...b29a</span><Icon name="chevron"/></button></div>
    </header>

    <div className="trade-main">
      <section className="market-overview">
        <div className="market-selector-wrap">
          <button className="market-selector" aria-expanded={marketMenu} onClick={() => { setMarketMenu(open => !open); setMarketSearch('') }}><span className="stock-avatar"><TickerIcon ticker={market.ticker} size={20}/></span><span className="market-selector-text"><b>{market.ticker}</b><small>{market.name} / US equity</small></span><Icon name="chevron"/></button>
          {marketMenu && <div className="market-menu"><label className="market-search"><Icon name="search"/><input autoFocus value={marketSearch} onChange={event => setMarketSearch(event.target.value)} placeholder="Search stocks" aria-label="Search stocks"/></label><div className="market-menu-list">{filteredMarkets.map((item) => { const i = markets.indexOf(item); return <button key={item.ticker} className={i === marketIndex ? 'selected' : ''} onClick={() => { setMarketIndex(i); setMarketMenu(false); setMarketSearch('') }}><span className="stock-avatar small"><TickerIcon ticker={item.ticker} size={17}/></span><span><b>{item.ticker}</b><small>{item.name} / US equity</small></span></button> })}{filteredMarkets.length === 0 && <p className="market-empty">No stocks found.</p>}</div></div>}
        </div>
        <div className="overview-price"><small className="metric-label" tabIndex={0} aria-describedby="tip-mark-price">Perp mark price<span className="metric-tooltip" id="tip-mark-price" role="tooltip">The reference price used to value open positions and help determine liquidations. It can differ from the price available to trade.</span></small><strong>Unavailable</strong><span>Awaiting market feed</span></div>
        <div className="overview-metric"><small className="metric-label" tabIndex={0} aria-describedby="tip-volume">24h volume<span className="metric-tooltip" id="tip-volume" role="tooltip">The total value of trades recorded in this market over the past 24 hours.</span></small><strong>Unavailable</strong></div>
        <div className="overview-metric"><small className="metric-label" tabIndex={0} aria-describedby="tip-open-interest">Open interest<span className="metric-tooltip" id="tip-open-interest" role="tooltip">The current total value of open positions in this market.</span></small><strong>Unavailable</strong></div>
        <div className="overview-metric funding"><small className="metric-label" tabIndex={0} aria-describedby="tip-funding">Funding / 8h<span className="metric-tooltip" id="tip-funding" role="tooltip">The funding rate applied for each 8-hour interval. Funding payments help keep the contract price aligned with its reference market.</span></small><strong>Unavailable</strong></div>
      </section>

      <div className="trading-grid" style={{ '--order-width': `${orderWidth}px`, '--chart-height': `${chartHeight}px` } as CSSProperties}>
        <section className="market-workspace" aria-label="Market chart and positions"><PriceChart market={market}/>
          <button className="resize-handle resize-horizontal" aria-label="Resize chart and positions panels" onPointerDown={event => startResize('y', event, chartHeight)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize}><span/></button>
          <div className="positions-panel">
            <div className="positions-heading"><div className="position-tabs">{['Positions', 'Open orders', 'History'].map(tab => <button key={tab} className={activeTab === tab ? 'active' : ''} onClick={() => setActiveTab(tab)}>{tab}{tab === 'Positions' && <span className="tab-count">2</span>}</button>)}</div></div>
            <div className="position-table-wrap"><table className="position-table"><thead><tr><th>Market</th><th>Side</th><th>Size</th><th>Margin</th><th>Leverage</th><th>Entry price</th><th>Mark price</th><th>Unrealized PnL</th><th/></tr></thead><tbody>
              <tr><td><span className="table-market"><span className="stock-avatar tiny"><TickerIcon ticker="NVDA" size={13}/></span><b>NVDA</b><small>US equity</small></span></td><td><span className="long-side">↗ Long</span></td><td>$1,240.00</td><td>$413.33</td><td>3.0×</td><td>$139.42</td><td>$142.87</td><td><span className="green-pnl">+$30.69 <small>(+7.43%)</small></span></td><td><button className="close-position" onClick={() => setNotice('Position closing will be available when trading is connected.')}>Close</button></td></tr>
              <tr><td><span className="table-market"><span className="stock-avatar tiny"><TickerIcon ticker="META" size={13}/></span><b>META</b><small>US equity</small></span></td><td><span className="short-side">↘ Short</span></td><td>$760.00</td><td>$380.00</td><td>2.0×</td><td>$719.10</td><td>$712.66</td><td><span className="green-pnl">+$6.81 <small>(+1.79%)</small></span></td><td><button className="close-position" onClick={() => setNotice('Position closing will be available when trading is connected.')}>Close</button></td></tr>
            </tbody></table></div>
            {notice && <div className="trade-notice" role="status">{notice}<button onClick={() => setNotice('')} aria-label="Dismiss">×</button></div>}
          </div>
        </section>
        <button className="resize-handle resize-vertical" aria-label="Resize order panel" onPointerDown={event => startResize('x', event, orderWidth)} onPointerMove={moveResize} onPointerUp={endResize} onPointerCancel={endResize}><span/></button>
        <aside className="order-card" aria-label="Place an order">
          <div className="order-card-heading"><h1>Place order</h1></div>
          <div className="side-switch"><button className={side === 'long' ? 'active long-active' : ''} onClick={() => setSide('long')}><span>↗</span> Long</button><button className={side === 'short' ? 'active short-active' : ''} onClick={() => setSide('short')}><span>↘</span> Short</button></div>
          <div className="order-types"><button className={orderType === 'market' ? 'active' : ''} onClick={() => setOrderType('market')}>Market</button><button className={orderType === 'limit' ? 'active' : ''} onClick={() => setOrderType('limit')}>Limit</button><button className="order-info" title="Market orders execute at the current market price.">i</button></div>
          {orderType === 'limit' && <label className="field-label">Limit price<div className="input-wrap"><span>$</span><input inputMode="decimal" placeholder="Enter limit price" aria-label="Limit price"/></div></label>}
          <label className="field-label">Order size<div className="input-wrap"><span>$</span><input inputMode="decimal" value={size} onChange={e => setSize(e.target.value)} aria-label="Order size"/><button className="input-unit">USDC <Icon name="chevron"/></button></div><small className="input-subtext">Notional size</small></label>
          <label className="field-label payment-label"><span>Margin</span><span className="available-label">Available <b>$1,240.00</b> <button onClick={() => setPayment('1240')}>Max</button></span><div className="input-wrap"><span>$</span><input inputMode="decimal" value={payment} onChange={e => setPayment(e.target.value)} aria-label="Margin amount"/><button className="input-unit">USDC <Icon name="chevron"/></button></div></label>
          <div className={`leverage-control ${side}`} style={{ '--leverage-progress': `${((leverage - 1) / 9) * 100}%` } as CSSProperties}><div className="leverage-heading"><span>Leverage</span><strong>{leverage.toFixed(1)}×</strong></div><input type="range" min="1" max="10" step="0.5" value={leverage} onChange={e => setLeverage(Number(e.target.value))} aria-label="Leverage"/><div className="range-labels">{[1, 3, 5, 7, 10].map(value => <span className={leverage >= value ? 'reached' : ''} key={value}>{value}×</span>)}</div></div>
          <label className="tp-sl-toggle"><input type="checkbox"/><span className="custom-check"/><span>Take profit / Stop loss</span><button type="button" title="Set automatic close prices">i</button></label>
          <div className="order-summary"><div><span>Entry price</span><b>See chart</b></div><div><span>Position value</span><b>${positionValue.toFixed(2)}</b></div><div><span>Liquidation price</span><b>Unavailable</b></div><div><span>Trading fee</span><b>Unavailable</b></div></div>
          <button className={`submit-order ${side}`} onClick={() => setNotice('Order preview only. Connect trading to submit.')}>{side === 'long' ? 'Open long' : 'Open short'} <span>${positionValue.toFixed(2)}</span></button>
          <p className="order-disclaimer">Orders are simulated in this preview.</p>
        </aside>
      </div>
    </div>
    <footer className="trade-statusbar"><InterfaceSpeed/><span className="status-spacer"/><button className="footer-link" onClick={() => setRiskOpen(true)}>Risk disclosure</button><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>

    {notificationsOpen && <><button className="drawer-backdrop" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}/><aside className="notification-drawer" aria-label="Notifications"><div className="drawer-heading"><div><h2>Notifications</h2><p>Recent account activity</p></div><button className="drawer-close" aria-label="Close notifications" onClick={() => setNotificationsOpen(false)}>×</button></div><div className="drawer-actions"><span>{notifications.length} {notifications.length === 1 ? 'notification' : 'notifications'}</span><button disabled={notifications.length === 0} onClick={() => setNotifications([])}>Clear all</button></div><div className="notification-list">{notifications.map(item => <article className="notification-item" key={item.id}><div className="notification-body"><div className="notification-title-row"><h3>{item.title}</h3><time>{item.time}</time></div><p>{item.message}</p></div><button className="notification-delete" aria-label="Delete notification" title="Delete notification" onClick={() => setNotifications(current => current.filter(notification => notification.id !== item.id))}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4 7h16M9 7V4h6v3m3 0-.8 13H6.8L6 7m4 4v5m4-5v5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/></svg></button></article>)}{notifications.length === 0 && <div className="notifications-empty"><span>✓</span><h3>All clear</h3><p>You have no notifications.</p></div>}</div></aside></>}

    {riskOpen && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) setRiskOpen(false) }}><section className="risk-modal" role="dialog" aria-modal="true" aria-labelledby="risk-title"><header className="risk-modal-header"><div><span className="risk-eyebrow">PLEASE READ BEFORE TRADING</span><h2 id="risk-title">Risk disclosure</h2><p>Understand the product and its risks before you trade.</p></div><button className="drawer-close" aria-label="Close risk disclosure" onClick={() => setRiskOpen(false)}>×</button></header><div className="risk-modal-content">{riskDisclosureSections.map(section => <section key={section.title}><h3>{section.title}</h3><p>{section.body}</p></section>)}</div><footer className="risk-modal-footer"><span>Questions? Visit our documentation.</span><button onClick={() => setRiskOpen(false)}>I understand</button></footer></section></div>}
  </main>
}
