import { createFileRoute } from '@tanstack/react-router'
import { useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import { useWalletSession, type WalletChain as AdapterChain } from '../lib/wallet-session'
import { useToasts } from '../lib/toasts'
import logo from '../assets/afterbell-logo.png'
import phantomIcon from '../assets/wallet-icons/phantom.jpeg'
import solflareIcon from '../assets/wallet-icons/solflare.jpeg'
import metamaskIcon from '../assets/wallet-icons/metamask.jpeg'
import coinbaseIcon from '../assets/wallet-icons/coinbase-wallet.png'
import hotIcon from '../assets/wallet-icons/hot-wallet.jpeg'
import nightlyIcon from '../assets/wallet-icons/nightly.jpeg'
import meteorIcon from '../assets/wallet-icons/meteor.png'
import rainbowIcon from '../assets/wallet-icons/rainbow.png'
import rabbyIcon from '../assets/wallet-icons/rabby.jpeg'
import solanaChainIcon from '../assets/wallet-icons/chain-icons/solana.jpeg'
import nearChainIcon from '../assets/wallet-icons/chain-icons/near.jpeg'
import ethereumChainIcon from '../assets/wallet-icons/chain-icons/etheruem.jpeg'
import './index.css'

export const Route = createFileRoute('/')({ component: AuthPage })

function WalletIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path d="M4.5 7.5h13a2 2 0 0 1 2 2v9h-13a2 2 0 0 1-2-2v-11a2 2 0 0 1 2-2h11" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M19.5 11h-4a2 2 0 0 0 0 4h4v-4Z" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
      <circle cx="15.5" cy="13" r=".7" fill="currentColor" />
    </svg>
  )
}

const walletCatalog = {
  Solana: [
    { name: 'Phantom', icon: phantomIcon },
    { name: 'Solflare', icon: solflareIcon },
    { name: 'MetaMask', icon: metamaskIcon },
    { name: 'Coinbase Wallet', icon: coinbaseIcon },
    { name: 'HOT Wallet', icon: hotIcon },
    { name: 'Nightly Wallet', icon: nightlyIcon },
  ],
  EVM: [
    { name: 'Phantom', icon: phantomIcon },
    { name: 'MetaMask', icon: metamaskIcon },
    { name: 'Rainbow', icon: rainbowIcon },
    { name: 'Rabby', icon: rabbyIcon },
    { name: 'Coinbase Wallet', icon: coinbaseIcon },
    { name: 'HOT Wallet', icon: hotIcon },
    { name: 'Nightly Wallet', icon: nightlyIcon },
  ],
  NEAR: [
    { name: 'HOT Wallet', icon: hotIcon },
    { name: 'Nightly Wallet', icon: nightlyIcon },
    { name: 'Meteor Wallet', icon: meteorIcon },
  ],
} as const

type WalletChain = keyof typeof walletCatalog

const chainDescriptions: Record<WalletChain, string> = {
  Solana: 'Connect with a Solana wallet',
  EVM: 'Connect with an Ethereum-compatible wallet',
  NEAR: 'Connect with a NEAR wallet',
}

const tiles = [
  ['circle', 0, 0], ['quarter', 1, 0], ['leaf', 2, 0], ['blank', 3, 0], ['blank', 4, 0], ['quarter-flip', 5, 0], ['ring', 6, 0],
  ['dots', 0, 1], ['diamond', 1, 1], ['blank', 2, 1], ['quarter', 3, 1], ['diamond-soft', 4, 1], ['blank', 5, 1], ['slash', 6, 1],
  ['triangle', 0, 2], ['blank', 1, 2], ['ring', 2, 2], ['arch', 3, 2], ['blank', 4, 2], ['slash-soft', 5, 2], ['blank', 6, 2],
  ['triangle-flip', 0, 3], ['dots-small', 1, 3], ['blank', 2, 3], ['quarter-flip', 3, 3], ['blank', 4, 3], ['diamond-pair', 5, 3], ['ring-soft', 6, 3],
  ['slash', 0, 4], ['blank', 1, 4], ['corner', 2, 4], ['blank', 3, 4], ['leaf', 4, 4], ['blank', 5, 4], ['bars', 6, 4],
  ['blank', 0, 5], ['quarter', 1, 5], ['blank', 2, 5], ['ring-soft', 3, 5], ['blank', 4, 5], ['triangle', 5, 5], ['triangle-flip', 6, 5],
  ['corner-flip', 0, 6], ['blank', 1, 6], ['diamond-soft', 2, 6], ['leaf', 3, 6], ['blank', 4, 6], ['dots', 5, 6], ['diamond-pair', 6, 6],
] as const

function MarketArtwork() {
  return (
    <svg className="market-art" viewBox="0 0 700 700" role="img" aria-label="Abstract geometric Afterbell artwork">
      <defs>
        <pattern id="tile-grid" width="100" height="100" patternUnits="userSpaceOnUse">
          <path d="M100 0H0V100" fill="none" stroke="#26303b" strokeWidth="1" />
        </pattern>
      </defs>
      <rect width="700" height="700" fill="#090d12" />
      {tiles.map(([shape, col, row], index) => (
        <g className="art-tile" key={`${shape}-${col}-${row}`} transform={`translate(${col * 100} ${row * 100})`}>
          <rect width="100" height="100" fill={index % 4 === 0 ? '#111821' : index % 3 === 0 ? '#0e141b' : '#0b1016'} />
          <g className="art-tile-shape"><Shape name={shape} /></g>
        </g>
      ))}
      <rect width="700" height="700" fill="url(#tile-grid)" pointerEvents="none" />
      <rect x="1" y="1" width="698" height="698" rx="2" fill="none" stroke="#27323c" pointerEvents="none" />
    </svg>
  )
}

type ShapeName = (typeof tiles)[number][0]

function Shape({ name }: { name: ShapeName }) {
  const tone = name === 'diamond' || name === 'slash' ? '#397fca' : '#85919e'
  const opacity = name === 'diamond' || name === 'slash' ? 0.84 : 0.48

  switch (name) {
    case 'circle': return <circle cx="50" cy="50" r="27" fill={tone} opacity={opacity} />
    case 'quarter': return <path d="M24 18h58v10a48 48 0 0 1-48 48H24V18Z" fill={tone} opacity={opacity} />
    case 'quarter-flip': return <path d="M18 18h58v58H66a48 48 0 0 1-48-48v-10Z" fill={tone} opacity={opacity} />
    case 'diamond': return <path d="m50 17 33 33-33 33-33-33 33-33Z" fill={tone} opacity={opacity} />
    case 'diamond-soft': return <><path d="m50 22 28 28-28 28-28-28 28-28Z" fill={tone} opacity={opacity * 0.65} /><path d="m50 37 13 13-13 13-13-13 13-13Z" fill="#0b1016" opacity=".9" /></>
    case 'dots': return <Dots count={2} />
    case 'dots-small': return <Dots count={3} />
    case 'ring': return <circle cx="50" cy="50" r="30" fill="none" stroke={tone} strokeWidth="13" opacity={opacity} />
    case 'ring-soft': return <><circle cx="50" cy="50" r="29" fill="none" stroke={tone} strokeWidth="13" opacity={opacity * 0.65} /><circle cx="50" cy="50" r="8" fill={tone} opacity={opacity} /></>
    case 'leaf': return <path d="M15 76C15 42 42 17 83 17 83 54 57 83 15 76Z" fill={tone} opacity={opacity} />
    case 'arch': return <><path d="M13 80V50a37 37 0 0 1 74 0v30H73V50a23 23 0 0 0-46 0v30H13Z" fill={tone} opacity={opacity} /><path d="M13 80h74" stroke="#0b1016" strokeWidth="5" /></>
    case 'slash': return <path d="M61 0h39L39 100H0L61 0Z" fill={tone} opacity={opacity} />
    case 'slash-soft': return <path d="M64 0h36L36 100H0L64 0Z" fill={tone} opacity={opacity * 0.55} />
    case 'triangle': return <path d="M0 0h100L0 100V0Z" fill={tone} opacity={opacity} />
    case 'triangle-flip': return <path d="M0 0h100v100L0 0Z" fill={tone} opacity={opacity} />
    case 'corner': return <path d="M0 0h50v50h50v50H0V0Z" fill={tone} opacity={opacity} />
    case 'corner-flip': return <path d="M0 0h100v100H50V50H0V0Z" fill={tone} opacity={opacity} />
    case 'diamond-pair': return <><path d="m32 20 30 30-30 30L2 50l30-30Z" fill={tone} opacity={opacity} /><path d="m70 20 30 30-30 30-30-30 30-30Z" fill={tone} opacity={opacity * 0.7} /></>
    case 'bars': return <><rect x="20" y="24" width="60" height="17" rx="8.5" fill={tone} opacity={opacity} /><rect x="20" y="59" width="60" height="17" rx="8.5" fill={tone} opacity={opacity} /></>
    case 'blank': return null
  }
}

function Dots({ count }: { count: 2 | 3 }) {
  const gap = count === 2 ? 30 : 23
  const radius = count === 2 ? 8 : 6
  const start = 50 - gap * (count - 1) / 2
  return <g fill="#85919e" opacity=".55">{Array.from({ length: count * count }, (_, i) => <circle key={i} cx={start + (i % count) * gap} cy={start + Math.floor(i / count) * gap} r={radius} />)}</g>
}

function AuthPage() {
  const navigate = useNavigate()
  const { session, connect, connecting } = useWalletSession()
  const { showToast, dismissToast } = useToasts()
  const [walletModalOpen, setWalletModalOpen] = useState(false)
  const [selectedChain, setSelectedChain] = useState<WalletChain | null>(null)
  const [transitionDirection, setTransitionDirection] = useState<'forward' | 'backward' | null>(null)

  useEffect(() => {
    if (session) {
      const firstTimeSetup = sessionStorage.getItem('afterbell.first-time-setup') === 'true' ||
        localStorage.getItem(`afterbell.onboarding-required.v1:${session.accountId}`) === 'true'
      if (firstTimeSetup) void navigate({ to: '/setting-up', replace: true })
      else void navigate({ to: '/trade', search: { market: undefined, side: undefined }, replace: true })
    }
  // A saved wallet is handled once at page entry. A fresh connection below
  // chooses either first-time setup or the core app explicitly.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (!walletModalOpen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setWalletModalOpen(false)
        setSelectedChain(null)
      }
    }
    document.addEventListener('keydown', onKeyDown)
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', onKeyDown)
      document.body.style.overflow = ''
    }
  }, [walletModalOpen])

  const closeWalletModal = () => {
    setWalletModalOpen(false)
    setSelectedChain(null)
    setTransitionDirection(null)
  }

  const showWallets = (chain: WalletChain) => {
    setTransitionDirection('forward')
    setSelectedChain(chain)
  }

  const showNetworks = () => {
    setTransitionDirection('backward')
    setSelectedChain(null)
  }

  const continueWithWallet = async (walletName: string) => {
    if (!selectedChain) return
    const pendingToast = showToast('waiting', 'Waiting for wallet approval and account setup…', 0)
    try {
      const chain = selectedChain === 'Solana' ? 'solana' : selectedChain === 'EVM' ? 'evm' : 'near'
      await connect(chain as AdapterChain, walletName)
      dismissToast(pendingToast)
      closeWalletModal()
      const firstTime = sessionStorage.getItem('afterbell.first-time-setup') === 'true'
      showToast('success', firstTime ? 'Wallet connected. Your account is ready to set up.' : 'Wallet connected.', 3500)
      if (firstTime) await navigate({ to: '/setting-up' })
      else await navigate({ to: '/trade', search: { market: undefined, side: undefined } })
    } catch (error) {
      dismissToast(pendingToast)
      showToast('error', error instanceof Error ? error.message : 'Could not connect wallet. Please try again.')
    }
  }

  return (
    <main className="auth-layout">
      <section className="auth-panel" aria-labelledby="auth-title">
        <a className="brand" href="/" aria-label="Afterbell home">
          <img className="brand-mark" src={logo} alt="" />
          <span>Afterbell</span>
        </a>

        <div className="auth-content">
          <h1 id="auth-title">Welcome to Afterbell</h1>
          <p className="auth-subtitle">Connect your wallet to continue.</p>
          <button className="connect-button" type="button" onClick={() => setWalletModalOpen(true)}>
            <WalletIcon />
            <span>Connect wallet</span>
          </button>
          <p className="wallet-note">Your wallet stays in your control.</p>
        </div>

        <footer className="auth-footer">© 2026 Afterbell</footer>
      </section>

      <aside className="art-panel" aria-label="Afterbell visual identity">
        <div className="art-glow" />
        <MarketArtwork />
      </aside>

      {walletModalOpen && (
        <div
          className="wallet-modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeWalletModal()
          }}
        >
          <section className={`wallet-modal${selectedChain ? ' wallet-modal-wallets' : ''}`} role="dialog" aria-modal="true" aria-labelledby="wallet-modal-title">
            <header className="wallet-modal-header">
              {selectedChain ? (
                <button className="wallet-modal-back" type="button" onClick={showNetworks} aria-label="Back to wallet networks">
                  <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m12.5 4.5-5.5 5.5 5.5 5.5" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" /></svg>
                </button>
              ) : <span className="wallet-modal-header-spacer" />}
              <button className="wallet-modal-close" type="button" onClick={closeWalletModal} aria-label="Close wallet selection">
                <svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></svg>
              </button>
            </header>

            <div
              key={selectedChain ?? 'networks'}
              className={`wallet-modal-content${transitionDirection ? ` wallet-modal-content-${transitionDirection}` : ''}`}
            >
              <h2 id="wallet-modal-title">{selectedChain ? `${selectedChain} wallets` : 'Choose your network'}</h2>
              <p className="wallet-modal-description">
                {selectedChain ? chainDescriptions[selectedChain] : 'Choose the network you want to connect with.'}
              </p>

              {selectedChain ? (
                <div className="wallet-options" aria-label={`${selectedChain} wallets`}>
                  {walletCatalog[selectedChain].map((wallet) => (
                    <button className="wallet-option" key={wallet.name} type="button" disabled={connecting} onClick={() => void continueWithWallet(wallet.name)}>
                      <img src={wallet.icon} alt="" />
                      <span>{wallet.name}</span>
                      <svg className="wallet-option-chevron" viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m7.5 4.5 5.5 5.5-5.5 5.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" /></svg>
                    </button>
                  ))}
                </div>
              ) : (
                <div className="chain-options">
                  {(Object.keys(walletCatalog) as WalletChain[]).map((chain) => (
                    <button className="chain-option" key={chain} type="button" onClick={() => showWallets(chain)}>
                      <img
                        className="chain-option-icon"
                        src={chain === 'Solana' ? solanaChainIcon : chain === 'EVM' ? ethereumChainIcon : nearChainIcon}
                        alt=""
                      />
                      <span className="chain-option-copy"><strong>{chain}</strong><small>{chainDescriptions[chain]}</small></span>
                      <svg className="wallet-option-chevron" viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m7.5 4.5 5.5 5.5-5.5 5.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" /></svg>
                    </button>
                  ))}
                </div>
              )}
            </div>
          </section>
        </div>
      )}
    </main>
  )
}
