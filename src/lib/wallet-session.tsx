import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Buffer } from 'buffer'
import bs58 from 'bs58'

export type WalletChain = 'solana' | 'evm' | 'near'
export type WalletSession = {
  chain: WalletChain
  walletName: string
  address: string
  accountId: string
}

type WalletContextValue = {
  session: WalletSession | null
  connecting: boolean
  connect: (chain: WalletChain, walletName: string) => Promise<WalletSession>
  disconnect: () => Promise<void>
}

type Owner = { SolanaEd25519: { public_key: number[] } } | { Ethereum: { address: number[] } } | { NearEd25519: { public_key: number[] } }
type Payload = { message: string; nonce: string; recipient: string }
let cleanupCurrentWallet: (() => void) | null = null
if (typeof globalThis.Buffer === 'undefined') globalThis.Buffer = Buffer
const SESSION_KEY = 'afterbell.wallet-session.v1'
const nearNetwork = import.meta.env.VITE_NEAR_NETWORK === 'mainnet' ? 'mainnet' : 'testnet'
const contractId = import.meta.env.VITE_A1_CONTRACT_ID || (nearNetwork === 'mainnet' ? 'a1.afterbell.near' : 'afterbellfx-dev-faucet.testnet')

async function post<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`/api${path}`, { method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'application/json' }, body: JSON.stringify(body) })
  const value = await response.json().catch(() => ({})) as { error?: { message?: string } }
  if (!response.ok) throw new Error(value.error?.message || `Request failed (${response.status})`)
  return value as T
}

function bytesHex(bytes: Uint8Array) { return `0x${Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')}` }
function bytesBase64(bytes: Uint8Array) {
  let binary = ''
  bytes.forEach(byte => { binary += String.fromCharCode(byte) })
  return btoa(binary)
}
function base64Bytes(value: string) { return Uint8Array.from(atob(value), char => char.charCodeAt(0)) }
function selectInjectedWallet(chain: 'evm' | 'solana', name: string): any {
  const w = window as any
  if (chain === 'evm') {
    const candidates = [w.ethereum, ...(w.ethereum?.providers || []), w.phantom?.ethereum, w.coinbaseWalletExtension].filter(Boolean)
    const normalizedName = name.toLowerCase()
    const match = candidates.find((provider: any) => {
      const info = `${provider.info?.name || ''} ${provider.info?.rdns || ''}`.toLowerCase()
      return normalizedName.includes('metamask') ? provider.isMetaMask || info.includes('metamask')
        : normalizedName.includes('phantom') ? provider.isPhantom || info.includes('phantom')
        : normalizedName.includes('coinbase') ? provider.isCoinbaseWallet || info.includes('coinbase')
        : normalizedName.includes('rabby') ? provider.isRabby || info.includes('rabby')
        : normalizedName.includes('rainbow') ? provider.isRainbow || info.includes('rainbow')
        : normalizedName.includes('hot') ? info.includes('hot wallet') || info.includes('hot-labs')
        : normalizedName.includes('nightly') ? provider.isNightly || info.includes('nightly')
        : false
    })
    if (!match) throw new Error(`${name} was not detected. Open Afterbell in the wallet browser or install its extension.`)
    return match
  }
  const candidates = [w.phantom?.solana, w.solflare, w.solana, w.coinbaseSolana, w.nightly?.solana].filter(Boolean)
  const n = name.toLowerCase()
  const match = candidates.find((provider: any) => {
    const info = `${provider.name || ''} ${provider.wallet?.name || ''}`.toLowerCase()
    return n.includes('phantom') ? provider.isPhantom || info.includes('phantom')
      : n.includes('solflare') ? provider.isSolflare || info.includes('solflare')
      : n.includes('coinbase') ? provider.isCoinbaseWallet || info.includes('coinbase')
      : n.includes('nightly') ? provider.isNightly || info.includes('nightly')
      : n.includes('hot') ? provider.isHotWallet || info.includes('hot')
      : false
  })
  if (!match) throw new Error(`${name} was not detected. Open Afterbell in the wallet browser or install its extension.`)
  return match
}

async function connectAndSign(chain: WalletChain, walletName: string) {
  let address: string
  let accountId: string
  let owner: Owner
  let sign: (payload: Payload) => Promise<Uint8Array>
  let watchWallet: (onChange: () => void) => () => void = () => () => undefined

  if (chain === 'evm') {
    const provider = selectInjectedWallet('evm', walletName)
    const [account] = await provider.request({ method: 'eth_requestAccounts' }) as string[]
    if (!account) throw new Error('The wallet did not return an account')
    address = account
    accountId = `ethereum:${account.toLowerCase()}`
    const addressBytes = Uint8Array.from(account.slice(2).match(/.{2}/g)!.map((part: string) => parseInt(part, 16)))
    owner = { Ethereum: { address: Array.from(addressBytes) } }
    sign = async payload => {
      const signature = await provider.request({ method: 'personal_sign', params: [bytesHex(new TextEncoder().encode(payload.message)), account] }) as string
      return Uint8Array.from(signature.slice(2).match(/.{2}/g)!.map((part: string) => parseInt(part, 16)))
    }
    watchWallet = onChange => {
      const changed = (accounts: string[]) => { if (!accounts?.[0] || accounts[0].toLowerCase() !== account.toLowerCase()) onChange() }
      provider.on?.('accountsChanged', changed)
      return () => provider.removeListener?.('accountsChanged', changed)
    }
  } else if (chain === 'solana') {
    const provider = selectInjectedWallet('solana', walletName)
    const response = await provider.connect()
    const publicKey = new Uint8Array(response.publicKey?.toBytes?.() || provider.publicKey?.toBytes?.())
    if (publicKey.length !== 32) throw new Error('The wallet returned an invalid Solana public key')
    address = bs58.encode(publicKey)
    accountId = `solana:${address}`
    owner = { SolanaEd25519: { public_key: Array.from(publicKey) } }
    sign = async payload => {
      if (!provider.signMessage) throw new Error(`${walletName} does not support message signing`)
      const signed = await provider.signMessage(new TextEncoder().encode(payload.message), 'utf8')
      return new Uint8Array(signed.signature || signed)
    }
    watchWallet = onChange => {
      const changed = (key: any) => {
        const next = key?.toBase58?.() || (key?.toBytes ? bs58.encode(key.toBytes()) : '')
        if (!next || next !== address) onChange()
      }
      provider.on?.('accountChanged', changed)
      return () => provider.removeListener?.('accountChanged', changed)
    }
  } else {
    const [{ setupWalletSelector }, { setupHotWallet }, { setupNightly }] = await Promise.all([
      import('@near-wallet-selector/core'), import('@near-wallet-selector/hot-wallet'), import('@near-wallet-selector/nightly'),
    ])
    const selector = await setupWalletSelector({ network: nearNetwork, modules: [setupHotWallet(), setupNightly()] })
    const walletId = walletName.toLowerCase().startsWith('hot') ? 'hot-wallet' : 'nightly'
    const wallet = await selector.wallet(walletId)
    let accounts = await wallet.getAccounts()
    if (!accounts.length) accounts = await (wallet.signIn as any)({ contractId, accounts: [] })
    const selected = accounts[0]
    if (!selected) throw new Error('The wallet did not return a NEAR account')
    address = selected.accountId
    accountId = `near:${address}`
    if (accountId.length > 64) throw new Error('This NEAR account name is too long for an Afterbell account record')
    let publicKeyText = selected.publicKey
    if (!publicKeyText) {
      if (!wallet.signMessage) throw new Error(`${walletName} does not support NEP-413 message signing`)
      const discoveryNonce = Buffer.from(crypto.getRandomValues(new Uint8Array(32)))
      const proof = await wallet.signMessage({ message: 'Connect this NEAR wallet to Afterbell', nonce: discoveryNonce, recipient: contractId })
      if (!proof?.publicKey || proof.accountId !== address) throw new Error('The NEAR wallet did not prove its active public key')
      publicKeyText = proof.publicKey
    }
    if (!publicKeyText.startsWith('ed25519:')) throw new Error('A1 owner registration currently requires an Ed25519 NEAR wallet key')
    const publicKey = bs58.decode(publicKeyText.slice(8))
    if (publicKey.length !== 32) throw new Error('The wallet returned an invalid NEAR public key')
    owner = { NearEd25519: { public_key: Array.from(publicKey) } }
    sign = async payload => {
      if (!wallet.signMessage) throw new Error(`${walletName} does not support NEP-413 message signing`)
      const result = await wallet.signMessage({ message: payload.message, nonce: Buffer.from(base64Bytes(payload.nonce)), recipient: payload.recipient })
      if (!result?.signature) throw new Error('The NEAR wallet did not return a signature')
      return base64Bytes(result.signature)
    }
    watchWallet = onChange => {
      const subscription = selector.store.observable.subscribe((state: any) => {
        const current = state.accounts?.find((item: any) => item.active)?.accountId
        if (current && current !== address) onChange()
      })
      return () => subscription.unsubscribe()
    }
  }

  const status = await post<{ registered: boolean; owner?: Owner }>('/v1/account/owner/status', { account_id: accountId })
  if (status.registered) {
    if (JSON.stringify(status.owner) !== JSON.stringify(owner)) throw new Error('This account is already linked to a different wallet key. Contact support to recover it.')
    cleanupCurrentWallet?.()
    cleanupCurrentWallet = watchWallet(() => { localStorage.removeItem(SESSION_KEY); window.location.assign('/') })
    return { session: { chain, walletName, address, accountId }, firstTime: false }
  }

  const expiresAt = Date.now() + 5 * 60_000
  const challenge = await post<{ payload: Payload }>('/v1/account/owner/payload', { account_id: accountId, owner, expires_at_ms: expiresAt })
  const signature = await sign(challenge.payload)
  await post('/v1/account/owner/register', { account_id: accountId, owner, expires_at_ms: expiresAt, signature: bytesBase64(signature) })
  cleanupCurrentWallet?.()
  cleanupCurrentWallet = watchWallet(() => { localStorage.removeItem(SESSION_KEY); window.location.assign('/') })
  return { session: { chain, walletName, address, accountId }, firstTime: true }
}

const WalletContext = createContext<WalletContextValue | null>(null)

export function WalletSessionProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<WalletSession | null>(() => {
    try { return JSON.parse(localStorage.getItem(SESSION_KEY) || 'null') as WalletSession | null } catch { return null }
  })
  const [connecting, setConnecting] = useState(false)
  const connect = useCallback(async (chain: WalletChain, walletName: string) => {
    setConnecting(true)
    try {
      const result = await connectAndSign(chain, walletName)
      setSession(result.session)
      localStorage.setItem(SESSION_KEY, JSON.stringify(result.session))
      sessionStorage.setItem('afterbell.first-time-setup', result.firstTime ? 'true' : 'false')
      return result.session
    } finally { setConnecting(false) }
  }, [])
  const disconnect = useCallback(async () => {
    cleanupCurrentWallet?.()
    cleanupCurrentWallet = null
    setSession(null)
    localStorage.removeItem(SESSION_KEY)
  }, [])
  useEffect(() => {
    if (session) localStorage.setItem(SESSION_KEY, JSON.stringify(session))
  }, [session])
  const value = useMemo(() => ({ session, connecting, connect, disconnect }), [session, connecting, connect, disconnect])
  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>
}

export function useWalletSession() {
  const context = useContext(WalletContext)
  if (!context) throw new Error('useWalletSession must be used inside WalletSessionProvider')
  return context
}
