import { createFileRoute, Link } from '@tanstack/react-router'
import { useMemo, useState } from 'react'
import logo from '../assets/afterbell-logo.png'
import { TickerIcon } from '../components/TickerIcon'
import { markets } from '../data/markets'
import './trade.css'
import './markets.css'

export const Route = createFileRoute('/markets')({ component: MarketsPage })

function MarketsPage() {
  const [query, setQuery] = useState('')
  const filteredMarkets = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return markets.filter(market => `${market.ticker} ${market.name} us equity robinhood chain`.toLowerCase().includes(normalizedQuery))
  }, [query])

  return <div className="trade-app markets-page">
    <header className="trade-header">
      <a href="/" className="trade-brand"><img className="trade-logo" src={logo} alt=""/><span>Afterbell</span></a>
      <nav className="trade-nav" aria-label="Main navigation"><a href="/trade">Trade</a><a className="nav-active" href="/markets">Markets</a><a href="#agent">AI agent</a></nav>
      <div className="header-right"><button className="account-button" onClick={() => window.location.assign('/')}><span className="account-avatar">D</span><span className="account-address">0x84...b29a</span></button></div>
    </header>

    <main className="markets-content">
      <div className="markets-title-row">
        <div><span className="markets-eyebrow">MARKETS</span><h1>Supported markets</h1><p>Explore the US equity markets available on Afterbell.</p></div>
        <div className="markets-total"><strong>{markets.length.toString().padStart(2, '0')}</strong><span>markets supported</span></div>
      </div>

      <section className="markets-directory" aria-label="Supported markets list">
        <div className="markets-directory-toolbar">
          <label className="markets-search"><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></svg><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search markets" aria-label="Search supported markets"/></label>
          <span>{filteredMarkets.length} {filteredMarkets.length === 1 ? 'market' : 'markets'}</span>
        </div>
        <div className="markets-table-head"><span>Market</span><span>Type</span><span>Network</span><span/></div>
        <div className="markets-rows">{filteredMarkets.map(market => <article className="supported-market-row" key={market.ticker}>
          <div className="supported-market-name"><span className="market-brand-icon"><TickerIcon ticker={market.ticker} size={20}/></span><span><b>{market.ticker}</b><small>{market.name}</small></span></div>
          <span className="market-type">US equity</span>
          <span className="market-network"><i/>Robinhood Chain</span>
          <Link className="market-open-link" to="/trade" search={{ market: market.ticker }}>View market <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="M4 10h11m-4-4 4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg></Link>
        </article>)}{filteredMarkets.length === 0 && <div className="markets-no-results">No supported markets match “{query}”.</div>}</div>
      </section>
      <p className="markets-footnote">Market availability and trading status may vary by network.</p>
    </main>

    <footer className="trade-statusbar"><span className="status-spacer"/><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
  </div>
}
