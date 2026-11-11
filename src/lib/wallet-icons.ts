import phantomIcon from '../assets/wallet-icons/phantom.jpeg'
import solflareIcon from '../assets/wallet-icons/solflare.jpeg'
import metamaskIcon from '../assets/wallet-icons/metamask.jpeg'
import coinbaseIcon from '../assets/wallet-icons/coinbase-wallet.png'
import rainbowIcon from '../assets/wallet-icons/rainbow.png'
import rabbyIcon from '../assets/wallet-icons/rabby.jpeg'
import hotIcon from '../assets/wallet-icons/hot-wallet.jpeg'
import nightlyIcon from '../assets/wallet-icons/nightly.jpeg'
import solanaIcon from '../assets/wallet-icons/chain-icons/solana.jpeg'
import evmIcon from '../assets/wallet-icons/chain-icons/etheruem.jpeg'
import nearIcon from '../assets/wallet-icons/chain-icons/near.jpeg'
import type { WalletChain } from './wallet-session'

const walletIcons: Record<string, string> = {
  'Phantom': phantomIcon,
  'Solflare': solflareIcon,
  'MetaMask': metamaskIcon,
  'Coinbase Wallet': coinbaseIcon,
  'Rainbow': rainbowIcon,
  'Rabby': rabbyIcon,
  'HOT Wallet': hotIcon,
  'Nightly Wallet': nightlyIcon,
}

export function getWalletIcon(walletName: string) {
  return walletIcons[walletName] ?? nightlyIcon
}

export function getChainIcon(chain: WalletChain) {
  return chain === 'near' ? nearIcon : chain === 'evm' ? evmIcon : solanaIcon
}

export function getChainName(chain: WalletChain) {
  return chain === 'near' ? 'NEAR' : chain === 'evm' ? 'EVM' : 'Solana'
}
