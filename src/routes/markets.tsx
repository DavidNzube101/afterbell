import { createFileRoute, Link } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'
import { siRobinhood } from 'simple-icons'
import logo from '../assets/afterbell-logo.png'
import { TickerIcon } from '../components/TickerIcon'
import { useMarketRegistry } from '../lib/api'
import { useWalletSession } from '../lib/wallet-session'
import './trade.css'
import './markets.css'

export const Route = createFileRoute('/markets')({ component: MarketsPage })

function MarketsPage() {
  const { session, disconnect } = useWalletSession()
  useEffect(() => { if (!session) window.location.assign('/') }, [session])
  const [query, setQuery] = useState('')
  const { markets, error } = useMarketRegistry()
  const filteredMarkets = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return markets.filter(market => `${market.ticker} ${market.name} us equity robinhood chain`.toLowerCase().includes(normalizedQuery))
  }, [query])

  return <div className="trade-app markets-page">
    <header className="trade-header">
      <a href="/" className="trade-brand"><img className="trade-logo" src={logo} alt=""/><span>Afterbell</span></a>
      <nav className="trade-nav" aria-label="Main navigation"><a href="/trade">Trade</a><a className="nav-active" href="/markets">Markets</a><a href="#agent">AI agent</a></nav>
      <div className="header-right"><button className="account-button" title={`${session?.walletName ?? 'Wallet'} · ${session?.chain ?? 'disconnected'} · Disconnect`} onClick={() => { void disconnect(); window.location.assign('/') }}><span className="account-avatar wallet-chain-avatar">{session?.chain === 'solana' ? 'SOL' : session?.chain === 'evm' ? 'EVM' : 'NEAR'}</span><span className="account-address">{session ? `${session.address.slice(0, 6)}...${session.address.slice(-4)}` : 'Connect wallet'}</span></button></div>
    </header>

    <main className="markets-content">
      <div className="markets-title-row">
        <div><h1>Markets</h1><p>Explore the US equity markets available on Afterbell.</p>{error && <p className="markets-api-error">Market data is temporarily unavailable. Showing the configured market list.</p>}</div>
      </div>

      <section className="markets-directory" aria-label="Markets">
        <div className="markets-directory-toolbar">
          <label className="markets-search"><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></svg><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search markets" aria-label="Search markets"/></label>
        </div>
        <div className="markets-table-head"><span>Market</span><span>Type</span><span>Network</span><span/></div>
        <div className="markets-rows">{filteredMarkets.map(market => <article className="supported-market-row" key={market.ticker}>
          <div className="supported-market-name"><span className="market-brand-icon"><TickerIcon ticker={market.ticker} size={20}/></span><span><b>{market.ticker}</b><small>{market.name}</small></span></div>
          <span className="market-type">{market.ticker}-USDC</span>
          <span className="market-network"><span className="robinhood-icon" role="img" aria-label="Robinhood Chain"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={siRobinhood.path}/></svg></span>Robinhood Chain</span>
          <div className="market-actions"><Link className="market-open-link long" to="/trade" search={{ market: market.ticker, side: 'long' }}>Long</Link><Link className="market-open-link short" to="/trade" search={{ market: market.ticker, side: 'short' }}>Short</Link></div>
        </article>)}{filteredMarkets.length === 0 && <div className="markets-no-results">No supported markets match “{query}”.</div>}</div>
      </section>
    </main>

    <footer className="trade-statusbar"><span className="status-spacer"/><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
  </div>
}
