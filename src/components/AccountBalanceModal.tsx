import { useEffect, useState } from 'react'
import QRCode from 'qrcode'
import type { WalletSession } from '../lib/wallet-session'
import { formatNear, formatUsdc } from '../lib/dashboard'
import { UsdcIcon } from './UsdcIcon'
import './account-balance-modal.css'

type Page = 'overview' | 'deposit' | 'withdraw'
type Props = {
  session: WalletSession
  balanceRaw: string | null
  nearBalanceRaw: string | null
  deposit: { contract_id: string; token_id: string; network: string } | null
  onClose: () => void
  onCopy: (value: string) => void
  onWithdraw: (amount: string, receiver: string) => Promise<void>
}

export function AccountBalanceModal({ session, balanceRaw, nearBalanceRaw, deposit, onClose, onCopy, onWithdraw }: Props) {
  const [page, setPage] = useState<Page>('overview')
  const [direction, setDirection] = useState<'next' | 'back'>('next')
  const [amount, setAmount] = useState('')
  const [receiver, setReceiver] = useState(session.chain === 'near' ? session.address : '')
  const [qr, setQr] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState('')
  const [depositId] = useState(() => typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`)
  const depositMessage = JSON.stringify({ account_id: session.accountId, deposit_id: depositId })
  const qrValue = deposit ? JSON.stringify({ network: deposit.network, token_id: deposit.token_id, receiver_id: deposit.contract_id, method: 'ft_transfer_call', msg: depositMessage }) : ''

  useEffect(() => {
    if (!qrValue) return
    let active = true
    void QRCode.toDataURL(qrValue, { width: 210, margin: 1, color: { dark: '#10161f', light: '#ffffff' } }).then(value => { if (active) setQr(value) })
    return () => { active = false }
  }, [qrValue])

  function go(next: Page) { setDirection(next === 'overview' ? 'back' : 'next'); setFormError(''); setPage(next) }
  async function submit(event: React.FormEvent) {
    event.preventDefault(); setFormError('')
    try {
      if (!amount || Number(amount) <= 0) throw new Error('Enter a withdrawal amount.')
      if (!receiver.trim()) throw new Error('Enter a NEAR account to receive USDC.')
      setSubmitting(true); await onWithdraw(amount, receiver.trim()); onClose()
    } catch (error) { setFormError(error instanceof Error ? error.message : 'Withdrawal failed.') }
    finally { setSubmitting(false) }
  }

  return <div className="balance-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose() }}>
    <section className="balance-modal" role="dialog" aria-modal="true" aria-labelledby="balance-modal-title">
      <header className="balance-modal-header">
        <div>{page !== 'overview' && <button className="balance-back" onClick={() => go('overview')} aria-label="Back">←</button>}<div><h2 id="balance-modal-title">{page === 'overview' ? 'Account balance' : page === 'deposit' ? 'Deposit USDC' : 'Withdraw USDC'}</h2><p>{page === 'overview' ? 'Your Afterbell account' : page === 'deposit' ? 'Fund your account on NEAR' : 'Send USDC to a NEAR account'}</p></div></div>
        <button className="balance-close" aria-label="Close balance modal" onClick={onClose}>×</button>
      </header>
      <div className={`balance-carousel ${direction}`} key={page}>
        {page === 'overview' && <div className="balance-overview-page">
          <div className="balance-main-value"><UsdcIcon size={28}/><div><span>USDC balance</span><strong>{balanceRaw === null ? 'Loading…' : `$${formatUsdc(balanceRaw)}`}</strong></div></div>
          <div className="balance-sub-value"><span>NEAR wallet balance</span><b>{session.chain === 'near' ? nearBalanceRaw === null ? 'Loading…' : `${formatNear(nearBalanceRaw)} NEAR` : 'Only shown for NEAR wallets'}</b></div>
          <div className="balance-actions"><button onClick={() => go('deposit')}><span>↓</span><b>Deposit</b></button><button onClick={() => go('withdraw')}><span>↑</span><b>Withdraw</b></button></div>
          <p className="balance-account-id">Afterbell account <b>{session.accountId}</b></p>
        </div>}
        {page === 'deposit' && <div className="balance-deposit-page">
          {deposit && <><div className="deposit-qr">{qr ? <img src={qr} alt="QR code with NEAR USDC transfer details"/> : <span>Generating QR…</span>}</div><p className="deposit-network">{deposit.network.toUpperCase()} · NEAR USDC</p><div className="deposit-address-label">A1 contract address</div><button className="deposit-copy-address" onClick={() => onCopy(deposit.contract_id)}>{deposit.contract_id}<span>Copy</span></button><div className="deposit-info"><span>Token contract</span><button onClick={() => onCopy(deposit.token_id)}>{deposit.token_id} <b>Copy</b></button></div><div className="deposit-info"><span>Transfer message</span><button onClick={() => onCopy(depositMessage)}>{depositMessage} <b>Copy</b></button></div><p className="deposit-warning">Use NEAR USDC <code>ft_transfer_call</code> with this message. A regular token transfer will not credit your Afterbell balance.</p></>}
          {!deposit && <p className="balance-error">Deposit details could not be loaded. Close this window and try again.</p>}
        </div>}
        {page === 'withdraw' && <form className="balance-withdraw-page" onSubmit={submit}>
          <div className="withdraw-available"><span>Available to withdraw</span><b>{balanceRaw === null ? 'Loading…' : `$${formatUsdc(balanceRaw)}`} USDC</b></div>
          <label>NEAR recipient account<input value={receiver} onChange={event => setReceiver(event.target.value)} placeholder="name.near" autoComplete="off"/></label>
          <label>Amount<div className="withdraw-amount-input"><input inputMode="decimal" value={amount} onChange={event => setAmount(event.target.value.replace(/[^0-9.]/g, ''))} placeholder="0.00"/><UsdcIcon size={18}/><span>USDC</span><button type="button" onClick={() => setAmount(balanceRaw === null ? '' : formatUsdc(balanceRaw))}>Max</button></div></label>
          {formError && <p className="balance-error" role="alert">{formError}</p>}
          <p className="withdraw-note">You’ll sign the withdrawal with your connected wallet. USDC will be sent on {deposit?.network ?? 'NEAR'}.</p>
          <button className="balance-primary-action" disabled={submitting || balanceRaw === null}>{submitting ? 'Submitting…' : 'Review and withdraw'}</button>
        </form>}
      </div>
    </section>
  </div>
}
