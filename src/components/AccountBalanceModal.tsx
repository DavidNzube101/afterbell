import { useEffect, useRef, useState } from 'react'
import { sendNearUsdcDeposit, type WalletSession } from '../lib/wallet-session'
import { formatNear, formatUsdc, usdcToRaw } from '../lib/dashboard'
import { UsdcIcon } from './UsdcIcon'
import hotIcon from '../assets/wallet-icons/hot-wallet.jpeg'
import nightlyIcon from '../assets/wallet-icons/nightly.jpeg'
import meteorIcon from '../assets/wallet-icons/meteor.png'
import nearChainIcon from '../assets/wallet-icons/chain-icons/near.jpeg'
import './account-balance-modal.css'

type Page = 'overview' | 'deposit' | 'deposit-wallet' | 'withdraw'
type Props = {
  session: WalletSession
  balanceRaw: string | null
  nearBalanceRaw: string | null
  deposit: { contract_id: string; token_id: string; network: string } | null
  onClose: () => void
  onCopy: (value: string) => void
  onWithdraw: (amount: string, receiver: string) => Promise<void>
  onDepositComplete: () => Promise<void>
}

const nearWallets = [
  { name: 'HOT Wallet', icon: hotIcon },
  { name: 'Nightly Wallet', icon: nightlyIcon },
  { name: 'Meteor Wallet', icon: meteorIcon },
]

export function AccountBalanceModal({ session, balanceRaw, nearBalanceRaw, deposit, onClose, onCopy, onWithdraw, onDepositComplete }: Props) {
  const [page, setPage] = useState<Page>('overview')
  const [direction, setDirection] = useState<'next' | 'back'>('next')
  const [amount, setAmount] = useState('')
  const depositAmountInput = useRef<HTMLInputElement>(null)
  const withdrawAmountInput = useRef<HTMLInputElement>(null)
  const withdrawReceiverInput = useRef<HTMLInputElement>(null)
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState('')
  const [depositResult, setDepositResult] = useState<{ wallet: string; address: string; transactionHash?: string } | null>(null)

  useEffect(() => {
    if (page !== 'deposit' && page !== 'withdraw') return
    const skipReactSelectionChangeForFormInputs = (event: Event) => {
      const activeInput = document.activeElement
      if (activeInput === depositAmountInput.current || activeInput === withdrawAmountInput.current || activeInput === withdrawReceiverInput.current) {
        event.stopImmediatePropagation()
      }
    }
    // Leave caret management to the browser and keep these input selections out of React's delegated event dispatcher.
    window.addEventListener('selectionchange', skipReactSelectionChangeForFormInputs, true)
    return () => window.removeEventListener('selectionchange', skipReactSelectionChangeForFormInputs, true)
  }, [page])

  function go(next: Page) { setDirection(next === 'overview' || (page === 'deposit-wallet' && next === 'deposit') ? 'back' : 'next'); setFormError(''); setPage(next) }
  async function submitDeposit(event: React.FormEvent) {
    event.preventDefault()
    setFormError('')
    try {
      if (!deposit) throw new Error('Deposit details could not be loaded. Try again shortly.')
      const entered = depositAmountInput.current?.value ?? ''
      const cleaned = entered.replace(/[^0-9.]/g, '')
      const [whole = '', ...decimalParts] = cleaned.split('.')
      const nextAmount = `${whole.slice(0, 18)}${decimalParts.length ? `.${decimalParts.join('').slice(0, 6)}` : ''}`
      if (depositAmountInput.current) depositAmountInput.current.value = nextAmount
      // Validate only on Continue so each keystroke stays outside React state.
      if (!/^\d+(\.\d{1,6})?$/.test(nextAmount) || !/[1-9]/.test(nextAmount)) {
        throw new Error('Enter a USDC amount greater than zero, with up to 6 decimals.')
      }
      setAmount(nextAmount)
      go('deposit-wallet')
    } catch (error) { setFormError(error instanceof Error ? error.message : 'Enter a valid deposit amount.') }
  }
  async function chooseDepositWallet(walletName: string) {
    if (!deposit || submitting) return
    setSubmitting(true)
    setFormError('')
    setDepositResult(null)
    try {
      const result = await sendNearUsdcDeposit(walletName, deposit.network, deposit.token_id, deposit.contract_id, session.accountId, usdcToRaw(amount.trim()))
      setDepositResult({ wallet: walletName, address: result.accountId, transactionHash: result.transactionHash })
      await onDepositComplete()
    } catch (error) { setFormError(error instanceof Error ? error.message : 'The USDC deposit did not complete.') }
    finally { setSubmitting(false) }
  }
  async function submit(event: React.FormEvent) {
    event.preventDefault(); setFormError('')
    try {
      const enteredAmount = withdrawAmountInput.current?.value.trim() ?? ''
      const receiver = withdrawReceiverInput.current?.value.trim() ?? ''
      if (!/^\d+(\.\d{1,6})?$/.test(enteredAmount) || Number(enteredAmount) <= 0) throw new Error('Enter a valid withdrawal amount, up to 6 decimals.')
      if (!receiver) throw new Error('Enter a NEAR account to receive USDC.')
      setSubmitting(true); await onWithdraw(enteredAmount, receiver); onClose()
    } catch (error) { setFormError(error instanceof Error ? error.message : 'Withdrawal failed.') }
    finally { setSubmitting(false) }
  }

  return <div className="balance-modal-backdrop" role="presentation" onMouseDown={event => { if (event.target === event.currentTarget) onClose() }}>
    <section className="balance-modal" role="dialog" aria-modal="true" aria-labelledby="balance-modal-title">
      <header className="balance-modal-header">
        <div>{page !== 'overview' && <button className="balance-back" onClick={() => go(page === 'deposit-wallet' ? 'deposit' : 'overview')} aria-label="Back">←</button>}<div><h2 id="balance-modal-title">{page === 'overview' ? 'Account balance' : page === 'deposit' || page === 'deposit-wallet' ? 'Deposit USDC' : 'Withdraw USDC'}</h2><p>{page === 'overview' ? 'Your Afterbell account' : page === 'deposit' ? 'Choose an amount to deposit' : page === 'deposit-wallet' ? 'Choose a NEAR wallet to continue' : 'Send USDC to a NEAR account'}</p></div></div>
        <button className="balance-close" aria-label="Close balance modal" onClick={onClose}>×</button>
      </header>
      <div className={`balance-carousel ${direction}`} key={page}>
        {page === 'overview' && <div className="balance-overview-page">
          <div className="balance-main-value"><UsdcIcon size={28}/><div><span>USDC balance</span><strong>{balanceRaw === null ? 'Loading…' : `$${formatUsdc(balanceRaw)}`}</strong></div></div>
          <div className="balance-sub-value"><span>NEAR wallet balance</span><b>{session.chain === 'near' ? nearBalanceRaw === null ? 'Loading…' : `${formatNear(nearBalanceRaw)} NEAR` : 'Only shown for NEAR wallets'}</b></div>
          <div className="balance-actions"><button onClick={() => go('deposit')}><span>↓</span><b>Deposit</b></button><button onClick={() => go('withdraw')}><span>↑</span><b>Withdraw</b></button></div>
          <p className="balance-account-id">Afterbell account <b>{session.accountId}</b></p>
        </div>}
        {page === 'deposit' && <form className="balance-deposit-page" onSubmit={submitDeposit}>
          <label className="deposit-amount-label">Amount to deposit<div className="deposit-amount-input"><input ref={depositAmountInput} autoFocus inputMode="decimal" maxLength={25} defaultValue={amount} placeholder="0.00" aria-label="USDC deposit amount"/><UsdcIcon size={19}/><span>USDC</span></div></label>
          <p className="deposit-note">Deposit NEAR USDC into your Afterbell account. You’ll choose a NEAR wallet to approve the transfer.</p>
          {formError && <p className="balance-error" role="alert">{formError}</p>}
          <button className="balance-primary-action" disabled={!deposit}>Continue</button>
        </form>}
        {page === 'deposit-wallet' && <div className="deposit-wallet-page">
          {depositResult ? <div className="deposit-complete" role="status"><span className="deposit-complete-icon">✓</span><h3>Deposit submitted</h3><p>{depositResult.wallet} signed the transfer from <b>{depositResult.address}</b>.</p>{depositResult.transactionHash && <button className="deposit-copy-address" onClick={() => onCopy(depositResult.transactionHash!)}>{depositResult.transactionHash}<span>Copy tx</span></button>}<p className="deposit-note">Your Afterbell USDC balance will update after the transfer is indexed.</p><button className="balance-primary-action" onClick={() => go('overview')}>Done</button></div> : <>
            <div className="deposit-network-heading"><img className="near-network-icon" src={nearChainIcon} alt="NEAR"/><div><b>NEAR wallets</b><small>Choose a wallet to sign this deposit</small></div></div>
            <div className="deposit-wallet-options">{nearWallets.map(wallet => <button key={wallet.name} className="deposit-wallet-option" disabled={submitting} onClick={() => void chooseDepositWallet(wallet.name)}><img src={wallet.icon} alt=""/><span>{wallet.name}</span><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m9 18 6-6-6-6" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"/></svg></button>)}</div>
            {submitting && <p className="deposit-progress" role="status"><span/>Waiting for wallet approval and transaction confirmation…</p>}
            {formError && <p className="balance-error" role="alert">{formError}</p>}
            <p className="deposit-selected-amount"><UsdcIcon size={18}/><b>{amount} USDC</b><span>to {session.accountId}</span></p>
            <p className="deposit-note">This wallet only approves the deposit. Your Afterbell login wallet stays connected.</p>
          </>}
        </div>}
        {page === 'withdraw' && <form className="balance-withdraw-page" onSubmit={submit}>
          <div className="withdraw-available"><span>Available to withdraw</span><b>{balanceRaw === null ? 'Loading…' : `$${formatUsdc(balanceRaw)}`} USDC</b></div>
          <label>NEAR recipient account<input ref={withdrawReceiverInput} defaultValue={session.chain === 'near' ? session.address : ''} placeholder="name.near" autoComplete="off"/></label>
          <label>Amount<div className="withdraw-amount-input"><input ref={withdrawAmountInput} inputMode="decimal" maxLength={25} placeholder="0.00"/><UsdcIcon size={18}/><span>USDC</span><button type="button" onClick={() => { if (withdrawAmountInput.current) withdrawAmountInput.current.value = balanceRaw === null ? '' : formatUsdc(balanceRaw) }}>Max</button></div></label>
          {formError && <p className="balance-error" role="alert">{formError}</p>}
          <p className="withdraw-note">You’ll sign the withdrawal with your connected wallet. USDC will be sent on {deposit?.network ?? 'NEAR'}.</p>
          <button className="balance-primary-action" disabled={submitting || balanceRaw === null}>{submitting ? 'Submitting…' : 'Review and withdraw'}</button>
        </form>}
      </div>
    </section>
  </div>
}
