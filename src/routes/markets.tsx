import { createFileRoute, Link } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'
import { siRobinhood } from 'simple-icons'
import logo from '../assets/afterbell-logo.png'
import { TickerIcon } from '../components/TickerIcon'
import { useMarketRegistry } from '../lib/api'
import { useWalletSession } from '../lib/wallet-session'
import { getMarketVolumes, getSnapshots, dollars, usdc, type MarketSnapshot } from '../lib/dashboard'
import { useToasts } from '../lib/toasts'
import phantomWalletIcon from '../assets/wallet-icons/phantom.jpeg'
import solflareWalletIcon from '../assets/wallet-icons/solflare.jpeg'
import metamaskWalletIcon from '../assets/wallet-icons/metamask.jpeg'
import coinbaseWalletIcon from '../assets/wallet-icons/coinbase-wallet.png'
import rainbowWalletIcon from '../assets/wallet-icons/rainbow.png'
import rabbyWalletIcon from '../assets/wallet-icons/rabby.jpeg'
import hotWalletIcon from '../assets/wallet-icons/hot-wallet.jpeg'
import nightlyWalletIcon from '../assets/wallet-icons/nightly.jpeg'
import solanaChainIcon from '../assets/wallet-icons/chain-icons/solana.jpeg'
import evmChainIcon from '../assets/wallet-icons/chain-icons/etheruem.jpeg'
import nearChainIcon from '../assets/wallet-icons/chain-icons/near.jpeg'
import './trade.css'
import './markets.css'

export const Route = createFileRoute('/markets')({ component: MarketsPage })

function MarketsPage() {
  const { session, disconnect } = useWalletSession()
  const { showToast } = useToasts()
  useEffect(() => { if (!session) window.location.assign('/') }, [session])
  const [query, setQuery] = useState('')
  const [walletModalOpen, setWalletModalOpen] = useState(false)
  const [snapshots, setSnapshots] = useState<MarketSnapshot[]>([])
  const [volumes, setVolumes] = useState<Record<string, string>>({})
  const { markets, error } = useMarketRegistry()
  useEffect(() => {
    const load = () => { void getSnapshots().then(setSnapshots).catch(() => undefined); void getMarketVolumes().then(rows => setVolumes(Object.fromEntries(rows.map(row => [row.market_id, row.volume_usdc_raw])))).catch(() => undefined) }
    load(); const timer = window.setInterval(load, 15000); return () => window.clearInterval(timer)
  }, [])
  const walletIcon = session?.walletName.toLowerCase().includes('phantom') ? phantomWalletIcon : session?.walletName.toLowerCase().includes('solflare') ? solflareWalletIcon : session?.walletName.toLowerCase().includes('metamask') ? metamaskWalletIcon : session?.walletName.toLowerCase().includes('coinbase') ? coinbaseWalletIcon : session?.walletName.toLowerCase().includes('rainbow') ? rainbowWalletIcon : session?.walletName.toLowerCase().includes('rabby') ? rabbyWalletIcon : session?.walletName.toLowerCase().includes('hot') ? hotWalletIcon : nightlyWalletIcon
  const chainIcon = session?.chain === 'solana' ? solanaChainIcon : session?.chain === 'evm' ? evmChainIcon : nearChainIcon
  const chainName = session?.chain === 'solana' ? 'Solana' : session?.chain === 'evm' ? 'EVM' : 'NEAR'
  const filteredMarkets = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase()
    return markets.filter(market => `${market.ticker} ${market.name} us equity robinhood chain`.toLowerCase().includes(normalizedQuery))
  }, [query])

  return <div className="trade-app markets-page">
    <header className="trade-header">
      <a href="/" className="trade-brand"><img className="trade-logo" src={logo} alt=""/><span>Afterbell</span></a>
      <nav className="trade-nav" aria-label="Main navigation"><a href="/trade">Trade</a><a className="nav-active" href="/markets">Markets</a><a href="#agent">AI agent</a></nav>
      <div className="header-right"><button className="account-button" title={`${session?.walletName ?? 'Wallet'} · ${chainName}`} onClick={() => setWalletModalOpen(true)}><img className="account-avatar wallet-chain-avatar connected-wallet-icon" src={walletIcon} alt=""/><span className="account-address">{session ? `${session.address.slice(0, 6)}...${session.address.slice(-4)}` : 'Connect wallet'}</span></button></div>
    </header>

    <main className="markets-content">
      <div className="markets-title-row">
        <div><h1>Markets</h1><p>Explore the US equity markets available on Afterbell.</p>{error && <p className="markets-api-error">Market data is temporarily unavailable. Showing the configured market list.</p>}</div>
      </div>

      <section className="markets-directory" aria-label="Markets">
        <div className="markets-directory-toolbar">
          <label className="markets-search"><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.8"/><path d="m16 16 4.5 4.5"/></svg><input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search markets" aria-label="Search markets"/></label>
        </div>
        <div className="markets-table-head"><span>Market</span><span>Type</span><span>Mark price</span><span>24h volume</span><span>Open interest</span><span>Funding / 8h</span><span>Network</span><span/></div>
        <div className="markets-rows">{filteredMarkets.map(market => <article className="supported-market-row" key={market.ticker}>
          <div className="supported-market-name"><span className="market-brand-icon"><TickerIcon ticker={market.ticker} size={20}/></span><span><b>{market.ticker}</b><small>{market.name}</small></span></div>
          <span className="market-type">{market.ticker}-USDC</span>
          <span>{snapshots.find(item => item.market_id === market.ticker)?.mark_price_e8 ? `$${dollars(snapshots.find(item => item.market_id === market.ticker)!.mark_price_e8).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` : 'Unavailable'}</span>
          <span>{volumes[market.ticker] ? `$${usdc(volumes[market.ticker]).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` : '$0.00'}</span>
          <span>{snapshots.find(item => item.market_id === market.ticker) ? `$${usdc(snapshots.find(item => item.market_id === market.ticker)!.open_interest_usdc).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` : 'Unavailable'}</span>
          <span>{snapshots.find(item => item.market_id === market.ticker) ? `${(snapshots.find(item => item.market_id === market.ticker)!.funding_rate_bps_8h / 100).toFixed(4)}%` : 'Unavailable'}</span>
          <span className="market-network"><span className="robinhood-icon" role="img" aria-label="Robinhood Chain"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={siRobinhood.path}/></svg></span>Robinhood Chain</span>
          <div className="market-actions"><Link className="market-open-link long" to="/trade" search={{ market: market.ticker, side: 'long' }}>Long</Link><Link className="market-open-link short" to="/trade" search={{ market: market.ticker, side: 'short' }}>Short</Link></div>
        </article>)}{filteredMarkets.length === 0 && <div className="markets-no-results">No supported markets match “{query}”.</div>}</div>
      </section>
    </main>

    <footer className="trade-statusbar"><span className="status-spacer"/><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
    {walletModalOpen && session && <div className="risk-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) setWalletModalOpen(false) }}><section className="wallet-detail-modal risk-modal" role="dialog" aria-modal="true" aria-labelledby="wallet-modal-title"><header className="risk-modal-header"><div><h2 id="wallet-modal-title">Connected wallet</h2></div><button className="drawer-close" aria-label="Close wallet details" onClick={() => setWalletModalOpen(false)}>×</button></header><div className="wallet-detail-content"><div className="wallet-detail-chain"><img src={chainIcon} alt=""/><span>{chainName} wallet</span></div><div className="wallet-detail-identity"><img src={walletIcon} alt=""/><span>{session.walletName}</span></div><div className="wallet-detail-address"><span>{session.address}</span><button onClick={() => void navigator.clipboard.writeText(session.address).then(() => showToast('success', 'Wallet address copied.')).catch(() => showToast('error', 'Could not copy the wallet address.'))}>Copy</button></div><button className="wallet-disconnect" onClick={() => { void disconnect(); window.location.assign('/') }}>Disconnect</button></div></section></div>}
  </div>
}
