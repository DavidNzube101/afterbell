import type { WalletSession } from '../lib/wallet-session'
import { getChainIcon, getChainName, getWalletIcon } from '../lib/wallet-icons'
import './wallet-details-modal.css'

type Props = {
  session: WalletSession
  onClose: () => void
  onCopy: (address: string) => void | Promise<void>
  onDisconnect: () => void | Promise<void>
}

export function WalletDetailsModal({ session, onClose, onCopy, onDisconnect }: Props) {
  return <div className="onboarding-wallet-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose() }}>
    <section className="onboarding-wallet-modal" role="dialog" aria-modal="true" aria-label="Wallet details">
      <header><button type="button" className="onboarding-wallet-close" aria-label="Close wallet details" onClick={onClose}>×</button></header>
      <dl>
        <div><dt>Chain</dt><dd className="onboarding-wallet-value"><img src={getChainIcon(session.chain)} alt="" />{getChainName(session.chain)}</dd></div>
        <div><dt>Wallet</dt><dd className="onboarding-wallet-value"><img src={getWalletIcon(session.walletName)} alt="" />{session.walletName}</dd></div>
        <div><dt>Address</dt><dd className="onboarding-wallet-address">{session.address}</dd></div>
      </dl>
      <footer>
        <button type="button" className="onboarding-wallet-copy" onClick={() => void onCopy(session.address)}>Copy address</button>
        <button type="button" className="onboarding-wallet-disconnect" onClick={() => void onDisconnect()}>Disconnect</button>
      </footer>
    </section>
  </div>
}
