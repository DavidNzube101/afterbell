import { createFileRoute } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useState } from 'react'
import logo from '../assets/afterbell-logo.png'
import { UsdcIcon } from '../components/UsdcIcon'
import { WalletDetailsModal } from '../components/WalletDetailsModal'
import { completeLpWithdrawal, formatUsdc, getLiquidityOverview, submitDeposit, submitLpWithdrawal, usdcToRaw, walletPayload, getDepositPayload, getLpWithdrawalPayload, type LiquidityOverview } from '../lib/dashboard'
import { getChainName, getWalletIcon } from '../lib/wallet-icons'
import { useToasts } from '../lib/toasts'
import { useWalletSession } from '../lib/wallet-session'
import './trade.css'
import './lp.css'

export const Route = createFileRoute('/lp')({ component: LiquidityPage })

const COOLDOWN_MS = 24 * 60 * 60 * 1000

function LiquidityPage() {
  const { session, disconnect, signMessage } = useWalletSession()
  const { showToast } = useToasts()
  const [walletModalOpen, setWalletModalOpen] = useState(false)
  const [mode, setMode] = useState<'add' | 'remove'>('add')
  const [amount, setAmount] = useState('')
  const [minimumOutput, setMinimumOutput] = useState('')
  const [overview, setOverview] = useState<LiquidityOverview | null>(null)
  const [loading, setLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [now, setNow] = useState(Date.now())
  const walletIcon = session ? getWalletIcon(session.walletName) : undefined
  const chainName = session ? getChainName(session.chain) : 'NEAR'
  const accountId = session?.accountId

  const refresh = useCallback(async (showError = true) => {
    if (!accountId) { setOverview(null); setLoading(false); return }
    try {
      setOverview(await getLiquidityOverview(accountId))
    } catch (error) {
      if (showError) showToast('error', error instanceof Error ? error.message : 'Could not load vault data.')
    } finally { setLoading(false) }
  }, [accountId, showToast])

  useEffect(() => {
    setLoading(true)
    void refresh()
    const pollWhenVisible = () => { if (document.visibilityState === 'visible') void refresh(false) }
    const timer = window.setInterval(pollWhenVisible, 30000)
    document.addEventListener('visibilitychange', pollWhenVisible)
    return () => {
      window.clearInterval(timer)
      document.removeEventListener('visibilitychange', pollWhenVisible)
    }
  }, [refresh])

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [])

  const estimatedShares = useMemo(() => {
    if (!overview || !amount || !/^\d+(\.\d{0,6})?$/.test(amount)) return '0.000000'
    try {
      const rawAmount = BigInt(usdcToRaw(amount))
      const nav = BigInt(overview.vault_nav_usdc || '0')
      const shares = BigInt(overview.total_lp_shares || '0')
      return formatUsdc(shares === 0n || nav === 0n ? rawAmount.toString() : (rawAmount * shares / nav).toString())
    } catch { return '0.000000' }
  }, [amount, overview])

  const estimatedUsdcRaw = useMemo(() => {
    if (!overview || !amount || !/^\d+(\.\d{0,6})?$/.test(amount)) return '0'
    try {
      const sharesIn = BigInt(usdcToRaw(amount))
      const sharesTotal = BigInt(overview.total_lp_shares || '0')
      const nav = BigInt(overview.vault_nav_usdc || '0')
      return sharesTotal === 0n ? '0' : (sharesIn * nav / sharesTotal).toString()
    } catch { return '0' }
  }, [amount, overview])

  useEffect(() => {
    if (mode !== 'remove' || !amount) return
    try {
      const estimate = BigInt(estimatedUsdcRaw)
      setMinimumOutput((estimate * 99n / 100n / 1_000_000n).toString() + '.' + (estimate * 99n / 100n % 1_000_000n).toString().padStart(6, '0'))
    } catch { setMinimumOutput('') }
  }, [estimatedUsdcRaw, mode, amount])

  async function copyWalletAddress(address: string) {
    try { await navigator.clipboard.writeText(address); showToast('success', 'Wallet address copied.') }
    catch { showToast('error', 'Could not copy the wallet address.') }
  }

  async function disconnectWallet() {
    await disconnect()
    window.location.assign('/')
  }

  async function addLiquidity() {
    if (!session || !accountId) { showToast('error', 'Connect your wallet first.'); return }
    try {
      const amountRaw = usdcToRaw(amount)
      if (BigInt(amountRaw) <= 0n) throw new Error('Enter an amount greater than zero.')
      if (BigInt(amountRaw) > BigInt(overview?.balance_usdc ?? '0')) throw new Error('Your available Afterbell USDC balance is too low.')
      setSubmitting(true); showToast('waiting', 'Approve the liquidity action in your wallet.')
      const payload = await getDepositPayload(accountId, amountRaw)
      const signature = await signMessage(walletPayload(payload.message, payload.nonce, payload.recipient))
      const result = await submitDeposit({ ...payload, signature })
      showToast('success', `Liquidity added. Transaction ${result.transaction_hash.slice(0, 10)}…`)
      setAmount(''); await refresh(false)
    } catch (error) { showToast('error', error instanceof Error ? error.message : 'Could not add liquidity.') }
    finally { setSubmitting(false) }
  }

  async function requestWithdrawal() {
    if (!session || !accountId) { showToast('error', 'Connect your wallet first.'); return }
    try {
      const sharesRaw = usdcToRaw(amount)
      const minRaw = usdcToRaw(minimumOutput || '0')
      if (BigInt(sharesRaw) <= 0n) throw new Error('Enter the LP shares to withdraw.')
      if (BigInt(sharesRaw) > BigInt(overview?.lp_shares ?? '0')) throw new Error('You do not have enough LP shares.')
      setSubmitting(true); showToast('waiting', 'Approve the withdrawal request in your wallet.')
      const payload = await getLpWithdrawalPayload(accountId, sharesRaw, minRaw)
      const signature = await signMessage(walletPayload(payload.message, payload.nonce, payload.recipient))
      const result = await submitLpWithdrawal({ ...payload, signature })
      showToast('success', `Withdrawal requested. Transaction ${result.transaction_hash.slice(0, 10)}…`)
      setAmount(''); await refresh(false)
    } catch (error) { showToast('error', error instanceof Error ? error.message : 'Could not request a withdrawal.') }
    finally { setSubmitting(false) }
  }

  async function completeWithdrawal() {
    if (!accountId) return
    try {
      setSubmitting(true); showToast('waiting', 'Completing your LP withdrawal.')
      const result = await completeLpWithdrawal(accountId)
      showToast('success', `Withdrawal completed. Transaction ${result.transaction_hash.slice(0, 10)}…`)
      await refresh(false)
    } catch (error) { showToast('error', error instanceof Error ? error.message : 'Could not complete withdrawal.') }
    finally { setSubmitting(false) }
  }

  const request = overview?.withdrawal_request ?? null
  const unlockAt = request ? request.requested_at_ms + COOLDOWN_MS : 0
  const cooldownRemaining = Math.max(0, unlockAt - now)
  const duration = `${Math.floor(cooldownRemaining / 3_600_000)}h ${Math.floor(cooldownRemaining % 3_600_000 / 60_000)}m`

  return <div className="trade-app lp-page">
    <header className="trade-header">
      <a href="/trade" className="trade-brand"><img className="trade-logo" src={logo} alt=""/><span>Afterbell</span></a>
      <nav className="trade-nav" aria-label="Main navigation"><a href="/trade">Trade</a><a href="/markets">Markets</a><a className="nav-active" href="/lp">LP</a><a href="#agent">AI agent</a></nav>
      <div className="header-right"><button className="account-button" title={`${session?.walletName ?? 'Wallet'} · ${chainName}`} onClick={() => session ? setWalletModalOpen(true) : window.location.assign('/')}><img className="account-avatar wallet-chain-avatar connected-wallet-icon" src={walletIcon} alt=""/><span className="account-address">{session ? `${session.address.slice(0, 6)}...${session.address.slice(-4)}` : 'Connect wallet'}</span></button></div>
    </header>

    <main className="lp-content">
      <section className="lp-heading"><div><h1>Provide liquidity. Share the risk.</h1><p>Deposit USDC from your Afterbell account into the shared vault and receive LP shares. Share value follows the vault’s net asset value.</p></div></section>

      <section className="lp-stats" aria-label="Vault overview">
        <article><span>Vault NAV</span><strong>{loading ? <i className="metric-skeleton"/> : `$${formatUsdc(overview?.vault_nav_usdc)}`}</strong><small>USDC</small></article>
        <article><span>Available liquidity</span><strong>{loading ? <i className="metric-skeleton"/> : `$${formatUsdc(overview?.available_liquidity_usdc)}`}</strong><small>USDC</small></article>
        <article><span>Your LP shares</span><strong>{loading ? <i className="metric-skeleton"/> : formatUsdc(overview?.lp_shares)}</strong><small>Share balance</small></article>
      </section>

      <section className="lp-workspace">
        <article className="lp-position-card">
          <div className="lp-section-heading"><div><h2>Your liquidity</h2><p>Your free balance, vault shares, and withdrawal request.</p></div></div>
          {!accountId ? <div className="lp-empty-state"><h3>Connect a wallet to view liquidity</h3><p>Your Afterbell account’s vault position will appear here.</p></div> : loading ? <div className="lp-empty-state"><i className="metric-skeleton"/></div> : <>
            <div className="lp-account-summary"><div><span>Available Afterbell balance</span><strong><UsdcIcon/> {formatUsdc(overview?.balance_usdc)} USDC</strong></div><div><span>Current share value</span><strong>$ {overview && BigInt(overview.total_lp_shares || '0') > 0n ? formatUsdc((BigInt(overview.lp_shares) * BigInt(overview.vault_nav_usdc) / BigInt(overview.total_lp_shares)).toString()) : '0.000000'} USDC</strong></div></div>
            {request ? <div className="lp-withdrawal-request"><div><strong>Withdrawal in cooldown</strong><span>{cooldownRemaining > 0 ? `Available in ${duration}` : 'Ready to complete'}</span></div><div><span>Shares requested</span><b>{formatUsdc(request.shares)}</b></div><div><span>Minimum USDC</span><b>{formatUsdc(request.min_amount_usdc)}</b></div><button className="lp-submit remove" disabled={submitting || cooldownRemaining > 0} onClick={() => void completeWithdrawal()}>{submitting ? 'Processing…' : 'Complete withdrawal'}</button></div> : <div className="lp-info-note"><span>i</span><p>LP withdrawals have a 24-hour cooldown. The final USDC amount uses the vault NAV when you complete the request and must meet your minimum output.</p></div>}
          </>}
        </article>

        <article className="lp-action-card">
          <div className="lp-mode-switch" role="tablist" aria-label="Liquidity action"><button className={mode === 'add' ? 'active' : ''} role="tab" aria-selected={mode === 'add'} onClick={() => { setMode('add'); setAmount('') }}>Add liquidity</button><button className={mode === 'remove' ? 'active' : ''} role="tab" aria-selected={mode === 'remove'} onClick={() => { setMode('remove'); setAmount('') }}>Remove liquidity</button></div>
          {mode === 'add' ? <>
            <div className="lp-form-heading"><h2>Add liquidity</h2><p>Move USDC from your Afterbell balance to the shared vault.</p></div>
            <label className="lp-field-label" htmlFor="lp-add-amount">Amount</label>
            <div className="lp-amount-input"><input id="lp-add-amount" inputMode="decimal" value={amount} onChange={event => setAmount(event.target.value.replace(/[^0-9.]/g, ''))} placeholder="0.00"/><span className="lp-token"><UsdcIcon/>USDC</span></div>
            <div className="lp-balance-line"><span>Available balance</span><b>{loading ? '…' : `${formatUsdc(overview?.balance_usdc)} USDC`}</b></div>
            <div className="lp-estimate"><span>Estimated LP shares</span><b>{estimatedShares}</b></div>
            <button className="lp-submit" disabled={!session || loading || submitting || !amount} onClick={() => void addLiquidity()}>{submitting ? 'Waiting for wallet…' : 'Add liquidity'}</button>
          </> : <>
            <div className="lp-form-heading"><h2>Remove liquidity</h2><p>Request to return LP shares to your Afterbell balance after the cooldown.</p></div>
            <label className="lp-field-label" htmlFor="lp-remove-amount">Shares to remove</label>
            <div className="lp-amount-input"><input id="lp-remove-amount" inputMode="decimal" value={amount} onChange={event => setAmount(event.target.value.replace(/[^0-9.]/g, ''))} placeholder="0.00"/><span className="lp-token">LP shares</span></div>
            <div className="lp-balance-line"><span>Shares available</span><b>{loading ? '…' : formatUsdc(overview?.lp_shares)}</b></div>
            <div className="lp-estimate"><span>Estimated USDC</span><b>{formatUsdc(estimatedUsdcRaw)}</b></div>
            <label className="lp-field-label lp-min-label" htmlFor="lp-min-output">Minimum USDC after cooldown</label>
            <div className="lp-amount-input lp-min-input"><input id="lp-min-output" inputMode="decimal" value={minimumOutput} onChange={event => setMinimumOutput(event.target.value.replace(/[^0-9.]/g, ''))} placeholder="0.00"/><span className="lp-token"><UsdcIcon/>USDC</span></div>
            <button className="lp-submit remove" disabled={!session || loading || submitting || !amount} onClick={() => void requestWithdrawal()}>{submitting ? 'Waiting for wallet…' : 'Request withdrawal'}</button>
          </>}
          {!session && <p className="lp-connection-note">Connect your wallet to add or remove liquidity.</p>}
        </article>
      </section>
    </main>

    <footer className="trade-statusbar"><span className="status-spacer"/><a className="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
    {walletModalOpen && session && <WalletDetailsModal session={session} onClose={() => setWalletModalOpen(false)} onCopy={copyWalletAddress} onDisconnect={disconnectWallet}/>}
  </div>
}
