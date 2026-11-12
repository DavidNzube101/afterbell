import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useRef, useState } from 'react'
import logo from '../assets/afterbell-logo.png'
import ottoLogo from '../assets/afterbell-otto-logo.png'
import { markets } from '../data/markets'
import { useWalletSession } from '../lib/wallet-session'
import { useToasts } from '../lib/toasts'
import { createAgentKey, type AgentPublicKey } from '../lib/agent-key'
import { checkAccountUsername, claimAccountUsername, getAgentSetupPayload, reclaimAccountUsername, submitAgentSetup } from '../lib/account'
import { WalletDetailsModal } from '../components/WalletDetailsModal'
import './setting-up.css'

export const Route = createFileRoute('/setting-up')({ component: SettingUpPage })

function UsernameField({
  accountId,
  valueRef,
  inputRef,
  initialValue,
}: {
  accountId: string
  valueRef: { current: string }
  inputRef: { current: HTMLInputElement | null }
  initialValue: string
}) {
  const saveUsernameDraft = () => {
    try {
      const key = `afterbell.onboarding-draft.v1:${accountId}`
      const draft = JSON.parse(localStorage.getItem(key) || '{}')
      localStorage.setItem(key, JSON.stringify({ ...draft, username: valueRef.current }))
    } catch { /* Draft persistence is best-effort. */ }
  }

  return <>
    <div className="username-input-wrap">
      <span>@</span>
      <input ref={inputRef} id="onboarding-username" placeholder="yourname" autoComplete="off" defaultValue={initialValue}
        onBlur={event => { valueRef.current = event.currentTarget.value; saveUsernameDraft() }} />
    </div>
    <p className="username-availability">3–20 characters: lowercase letters, numbers, and underscores.</p>
  </>
}

function Icon({ kind, size = 16 }: { kind: 'check' | 'left' | 'right' | 'down'; size?: number }) {
  const paths = {
    check: <path d="m5 12 4 4L19 6"/>,
    left: <><path d="M19 12H5"/><path d="m12 19-7-7 7-7"/></>,
    right: <><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></>,
    down: <path d="m6 9 6 6 6-6"/>,
  }
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[kind]}</svg>
}

function SettingUpPage() {
  const navigate = useNavigate()
  const { session, disconnect, signMessage } = useWalletSession()
  const { showToast, dismissToast } = useToasts()
  const [draft] = useState(() => {
    try {
      const accountId = session?.accountId
      return accountId ? JSON.parse(localStorage.getItem(`afterbell.onboarding-draft.v1:${accountId}`) || 'null') : null
    } catch { return null }
  })
  const [step, setStep] = useState(0)
  const [walletModalOpen, setWalletModalOpen] = useState(false)
  const [usernameMode, setUsernameMode] = useState<'new' | 'reclaim'>(draft?.usernameMode === 'reclaim' ? 'reclaim' : 'new')
  const [selectedMarkets, setSelectedMarkets] = useState<string[]>(draft?.selectedMarkets ?? ['NVDA', 'AAPL'])
  const [leverage, setLeverage] = useState<number>(draft?.leverage ?? 3)
  const usernameRef = useRef<string>(draft?.username ?? '')
  const initialUsernameRef = useRef<string>(draft?.username ?? '')
  const usernameInputRef = useRef<HTMLInputElement>(null)
  const reclaimEmailInputRef = useRef<HTMLInputElement>(null)
  const agentNameInputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    const skipReactSelectionChangeForFormInputs = (event: Event) => {
      const activeInput = document.activeElement
      if (activeInput === usernameInputRef.current || activeInput === reclaimEmailInputRef.current || activeInput === agentNameInputRef.current) {
        event.stopImmediatePropagation()
      }
    }
    window.addEventListener('selectionchange', skipReactSelectionChangeForFormInputs, true)
    return () => window.removeEventListener('selectionchange', skipReactSelectionChangeForFormInputs, true)
  }, [])
  const [positionLimit, setPositionLimit] = useState<string>(draft?.positionLimit ?? '1000')
  const [dailyLossLimit, setDailyLossLimit] = useState<string>(draft?.dailyLossLimit ?? '250')
  const [accessDuration, setAccessDuration] = useState<string>(draft?.accessDuration ?? '30')
  const [agentRecordId, setAgentRecordId] = useState<string>(draft?.agentRecordId ?? '')
  const [agentPublicKey, setAgentPublicKey] = useState<AgentPublicKey | null>(draft?.agentPublicKey ?? null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [isClaimingUsername, setIsClaimingUsername] = useState(false)

  useEffect(() => {
    if (!session) void navigate({ to: '/', replace: true })
  }, [session, navigate])

  useEffect(() => {
    if (!session || (sessionStorage.getItem('afterbell.first-time-setup') !== 'true' &&
        localStorage.getItem(`afterbell.onboarding-required.v1:${session.accountId}`) !== 'true')) return
    const timer = window.setTimeout(() => {
      localStorage.setItem(`afterbell.onboarding-draft.v1:${session.accountId}`, JSON.stringify({
        usernameMode, selectedMarkets, leverage, username: usernameInputRef.current?.value ?? usernameRef.current,
        reclaimEmail: reclaimEmailInputRef.current?.value ?? draft?.reclaimEmail ?? '',
        agentName: agentNameInputRef.current?.value ?? draft?.agentName ?? 'My agent', positionLimit, dailyLossLimit, accessDuration,
        agentRecordId, agentPublicKey,
      }))
    }, 400)
    return () => window.clearTimeout(timer)
  }, [session, usernameMode, selectedMarkets, leverage, positionLimit, dailyLossLimit, accessDuration, agentRecordId, agentPublicKey])

  useEffect(() => {
    if (!walletModalOpen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setWalletModalOpen(false)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [walletModalOpen])

  const toggleMarket = (ticker: string) => setSelectedMarkets(current =>
    current.includes(ticker) ? current.filter(item => item !== ticker) : [...current, ticker],
  )

  const copyAddress = async (address: string) => {
    try {
      await navigator.clipboard.writeText(address)
      showToast('success', 'Wallet address copied.')
    } catch {
      showToast('error', 'Could not copy the wallet address.')
    }
  }

  const saveDraftField = (field: 'reclaimEmail' | 'agentName', value: string) => {
    if (!session) return
    try {
      const key = `afterbell.onboarding-draft.v1:${session.accountId}`
      const saved = JSON.parse(localStorage.getItem(key) || '{}')
      localStorage.setItem(key, JSON.stringify({ ...saved, [field]: value }))
    } catch { /* Draft persistence is best-effort. */ }
  }

  const disconnectWallet = async () => {
    await disconnect()
    setWalletModalOpen(false)
    await navigate({ to: '/', replace: true })
  }

  const continueSetup = async () => {
    if (!session || isClaimingUsername) return
    const reclaimEmail = reclaimEmailInputRef.current?.value ?? ''
    const username = (usernameInputRef.current?.value ?? usernameRef.current).trim()
    usernameRef.current = username
    if (usernameMode === 'reclaim') {
      if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(reclaimEmail.trim())) {
        showToast('error', 'Enter the email address used for your waitlist claim.')
        return
      }
    } else if (!/^[a-z0-9_]{3,20}$/.test(usernameRef.current.trim())) {
      showToast('error', 'Use 3–20 lowercase letters, numbers, or underscores for your username.')
      return
    }
    setIsClaimingUsername(true)
    const waitingToast = showToast('waiting', usernameMode === 'reclaim' ? 'Looking up your waitlist username…' : 'Checking username availability…', 0)
    try {
      if (usernameMode === 'reclaim') {
        const result = await reclaimAccountUsername({ account_id: session.accountId, email: reclaimEmail.trim() })
        usernameRef.current = result.username
        setStep(1)
        dismissToast(waitingToast)
        showToast('success', `Welcome back, ${result.username}.`)
      } else {
        const username = usernameRef.current.trim()
        const availability = await checkAccountUsername(session.accountId, username)
        if (!availability.valid || !availability.available) {
          throw new Error('That username is taken or reserved. Choose another one.')
        }
        const result = await claimAccountUsername({ account_id: session.accountId, username })
        usernameRef.current = result.username
        setStep(1)
        dismissToast(waitingToast)
        showToast('success', result.status === 'already_claimed' ? 'Your username is ready.' : 'Username claimed.')
      }
    } catch (error) {
      dismissToast(waitingToast)
      showToast('error', error instanceof Error ? error.message : 'Could not claim that username.')
    } finally {
      setIsClaimingUsername(false)
    }
  }

  const finishSetup = async () => {
    if (!session) return
    if (isSubmitting) return
    const agentName = agentNameInputRef.current?.value ?? 'My agent'
    const reclaimEmail = reclaimEmailInputRef.current?.value ?? ''
    const name = agentName.trim()
    if (!name) {
      showToast('error', 'Give your agent a name to continue.')
      return
    }
    if (selectedMarkets.length === 0) {
      showToast('error', 'Choose at least one market for your agent.')
      return
    }
    setIsSubmitting(true)
    const waitingToast = showToast('waiting', 'Waiting for your wallet to approve the agent permissions…', 0)
    try {
      const recordId = agentRecordId || `agent:${session.accountId}:${crypto.randomUUID().replaceAll('-', '')}`
      const publicKey = agentPublicKey ?? await createAgentKey(session.accountId, recordId)
      setAgentRecordId(recordId)
      setAgentPublicKey(publicKey)
      const draftState = {
        usernameMode, selectedMarkets, leverage, username: usernameRef.current, reclaimEmail, agentName, positionLimit, dailyLossLimit,
        accessDuration, agentRecordId: recordId, agentPublicKey: publicKey,
      }
      localStorage.setItem(`afterbell.onboarding-draft.v1:${session.accountId}`, JSON.stringify(draftState))
      const usdcUnits = (value: string) => (BigInt(value) * 1_000_000n).toString()
      const setup = {
        account_id: session.accountId,
        agent_record_id: recordId,
        agent_key: publicKey,
        allowed_markets: selectedMarkets,
        max_position_usdc: usdcUnits(positionLimit),
        max_open_notional_usdc: usdcUnits(positionLimit),
        max_leverage_bps: leverage * 10_000,
        max_allocation_usdc: usdcUnits(positionLimit),
        max_daily_loss_usdc: usdcUnits(dailyLossLimit),
        access_duration_days: Number(accessDuration),
      }
      const challenge = await getAgentSetupPayload(setup)
      const signature = await signMessage(challenge.payload)
      const result = await submitAgentSetup({
        ...setup,
        signature,
        expires_at_ms: challenge.expires_at_ms,
        nonce: challenge.nonce,
      })
      localStorage.setItem(`afterbell.onboarding.v1:${session.accountId}`, JSON.stringify({
        username: usernameRef.current.trim(), agent: { name, markets: selectedMarkets, maxLeverage: leverage,
          positionLimitUsdc: Number(positionLimit), dailyLossLimitUsdc: Number(dailyLossLimit),
          accessDurationDays: Number(accessDuration), recordId: result.agent_record_id,
          transactionHash: result.transaction_hash ?? null },
      }))
      localStorage.removeItem(`afterbell.onboarding-draft.v1:${session.accountId}`)
      localStorage.removeItem(`afterbell.onboarding-required.v1:${session.accountId}`)
      sessionStorage.removeItem('afterbell.first-time-setup')
      dismissToast(waitingToast)
      showToast('success', 'Your agent is set up. Welcome to Afterbell.')
      await navigate({ to: '/trade', search: { market: undefined, side: undefined } })
    } catch (error) {
      dismissToast(waitingToast)
      showToast('error', error instanceof Error ? error.message : 'Could not create your agent. Please try again.')
    } finally {
      setIsSubmitting(false)
    }
  }

  if (!session) return null

  return <main className="onboarding-screen">
    <header className="onboarding-header">
      <a className="onboarding-brand" href="/" aria-label="Afterbell home">
        <img src={logo} alt="" />
        <span>Afterbell</span>
      </a>
      <button className="onboarding-account" type="button" title="Wallet details" onClick={() => setWalletModalOpen(true)}>
        {session.address.slice(0, 6)}…{session.address.slice(-4)}
      </button>
    </header>

    <section className="onboarding-main" aria-label="Account setup">
      <div className="onboarding-intro">
        <h1>Let’s set things up.</h1>
        <p>Choose how you want to show up, then shape the permissions for your trading agent.</p>
      </div>

      <div className="onboarding-progress" aria-label={`Step ${step + 1} of 2`}>
        <div className="onboarding-progress-track"><span style={{ transform: `translateX(${step * 100}%)` }} /></div>
        <div className="onboarding-step-labels">
          <span className={step === 0 ? 'is-current' : 'is-done'}><b>{step > 0 ? <Icon kind="check" size={13} /> : '01'}</b> Username</span>
          <span className={step === 1 ? 'is-current' : ''}><b>02</b> Agent</span>
        </div>
      </div>

      <div className="onboarding-carousel-viewport">
        <div className="onboarding-carousel" style={{ transform: `translateX(-${step * 100}%)` }}>
          <article className="onboarding-slide" aria-hidden={step !== 0}>
            <div className="onboarding-card-heading">
              <div className="onboarding-icon-box"><span>@</span></div>
              <div><h2>Choose your username</h2><p>This is how people will find you on Afterbell.</p></div>
            </div>

            <div className="username-mode-switch" role="tablist" aria-label="Username option">
              <button className={usernameMode === 'new' ? 'active' : ''} role="tab" aria-selected={usernameMode === 'new'} onClick={() => setUsernameMode('new')}>Choose a username</button>
              <button className={usernameMode === 'reclaim' ? 'active' : ''} role="tab" aria-selected={usernameMode === 'reclaim'} onClick={() => setUsernameMode('reclaim')}>Reclaim waitlist name</button>
            </div>

            {usernameMode === 'new' ? <div className="username-form-preview">
              <label htmlFor="onboarding-username">Username</label>
              {session && <UsernameField accountId={session.accountId} valueRef={usernameRef} inputRef={usernameInputRef} initialValue={initialUsernameRef.current} />}
            </div> : <div className="username-form-preview reclaim-preview">
              <label htmlFor="reclaim-email">Email used for your waitlist claim</label>
              <div className="username-input-wrap"><input ref={reclaimEmailInputRef} id="reclaim-email" type="email" placeholder="you@example.com" autoComplete="email" defaultValue={draft?.reclaimEmail ?? ''} onBlur={event => saveDraftField('reclaimEmail', event.currentTarget.value)} /></div>
              <p>We’ll look up the username you reserved with this email.</p>
            </div>}

            <div className="onboarding-note"><span>Your wallet stays in control of your account.</span></div>
          </article>

          <article className="onboarding-slide agent-slide" aria-hidden={step !== 1}>
            <div className="onboarding-card-heading">
              <div className="onboarding-icon-box"><img className="agent-otto-logo" src={ottoLogo} alt="" /></div>
              <div><h2>Set up your agent</h2><p>Give it a name and choose what it’s allowed to do.</p></div>
            </div>

            <div className="agent-name-field"><label htmlFor="agent-name">Agent name</label><input ref={agentNameInputRef} id="agent-name" placeholder="My trading agent" defaultValue={draft?.agentName ?? 'My agent'} onBlur={event => saveDraftField('agentName', event.currentTarget.value)} /></div>
            <div className="agent-markets-field">
              <div className="agent-field-title"><label>Markets it can trade</label><span>{selectedMarkets.length} selected</span></div>
              <div className="agent-market-options">{markets.map(market => <button type="button" key={market.ticker} className={selectedMarkets.includes(market.ticker) ? 'selected' : ''} onClick={() => toggleMarket(market.ticker)} aria-pressed={selectedMarkets.includes(market.ticker)}><span>{market.ticker}-USDC</span>{selectedMarkets.includes(market.ticker) && <Icon kind="check" size={14} />}</button>)}</div>
            </div>
            <div className="agent-limits-grid">
              <label className="agent-limit-control"><span>Maximum leverage</span><strong>{leverage}×</strong><input aria-label="Maximum leverage" type="range" min="1" max="5" value={leverage} onChange={event => setLeverage(Number(event.target.value))} /></label>
              <label className="agent-limit-control"><span>Position limit</span><select aria-label="Position limit" value={positionLimit} onChange={event => setPositionLimit(event.target.value)}><option value="250">$250</option><option value="500">$500</option><option value="1000">$1,000</option><option value="2500">$2,500</option><option value="5000">$5,000</option></select></label>
              <label className="agent-limit-control"><span>Daily loss limit</span><select aria-label="Daily loss limit" value={dailyLossLimit} onChange={event => setDailyLossLimit(event.target.value)}><option value="50">$50</option><option value="100">$100</option><option value="250">$250</option><option value="500">$500</option></select></label>
              <label className="agent-limit-control"><span>Access duration</span><select aria-label="Access duration" value={accessDuration} onChange={event => setAccessDuration(event.target.value)}><option value="1">1 day</option><option value="7">7 days</option><option value="30">30 days</option></select></label>
            </div>
            <div className="onboarding-note"><span>The contract enforces these limits on every agent trade.</span></div>
          </article>
        </div>
      </div>

      <div className="onboarding-controls">
        <button className="onboarding-back" type="button" onClick={() => setStep(0)} disabled={step === 0}><Icon kind="left" size={16} /> Back</button>
        <div className="onboarding-dots" aria-hidden="true"><span className={step === 0 ? 'active' : ''} /><span className={step === 1 ? 'active' : ''} /></div>
        <button className="onboarding-next" type="button" onClick={step === 0 ? () => void continueSetup() : () => void finishSetup()} disabled={isSubmitting || isClaimingUsername}>{step === 0 ? isClaimingUsername ? 'Checking…' : <>Continue <Icon kind="right" size={16} /></> : isSubmitting ? 'Setting up…' : 'Finish setup'}</button>
      </div>
      <p className="onboarding-step-footnote">{step === 0 ? 'You can update your profile details later.' : 'Agent permissions can be adjusted whenever you need.'}</p>
    </section>

    {walletModalOpen && session && <WalletDetailsModal session={session} onClose={() => setWalletModalOpen(false)} onCopy={copyAddress} onDisconnect={disconnectWallet} />}
  </main>
}
