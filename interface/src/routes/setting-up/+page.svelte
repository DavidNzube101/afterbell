<script lang="ts">
	import { onMount } from 'svelte'
	import { goto } from '$app/navigation'
	import { markets } from '../../lib/market-data'
	import { initializeWalletSession, walletSession, signWalletMessage } from '../../lib/wallet-core'
	import { showToast, dismissToast } from '../../lib/toasts'
	import WalletDetailsModal from '../../lib/WalletDetailsModal.svelte'
	import { createAgentKey, type AgentPublicKey } from '../../lib/agent-key'
	import { checkAccountUsername, claimAccountUsername, getAgentSetupPayload, reclaimAccountUsername, submitAgentSetup } from '../../lib/account'

	let session = $derived($walletSession)
	let draft: Record<string, any> = {}
	let step = $state(0)
	let walletModalOpen = $state(false)
	let usernameMode = $state<'new' | 'reclaim'>('new')
	let selectedMarkets = $state<string[]>(['NVDA', 'AAPL'])
	let leverage = $state(3)
	let username = $state('')
	let reclaimEmail = $state('')
	let agentName = $state('My agent')
	let positionLimit = $state('1000')
	let dailyLossLimit = $state('250')
	let accessDuration = $state('30')
	let agentRecordId = $state('')
	let agentPublicKey = $state<AgentPublicKey | null>(null)
	let isSubmitting = $state(false)
	let isClaimingUsername = $state(false)

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) { void goto('/', { replaceState: true }); return }
		const key = `afterbell.onboarding-draft.v1:${$walletSession.accountId}`
		try { draft = JSON.parse(localStorage.getItem(key) || '{}') } catch { draft = {} }
		usernameMode = draft.usernameMode === 'reclaim' ? 'reclaim' : 'new'
		selectedMarkets = draft.selectedMarkets ?? ['NVDA', 'AAPL']
		leverage = draft.leverage ?? 3
		username = draft.username ?? ''
		reclaimEmail = draft.reclaimEmail ?? ''
		agentName = draft.agentName ?? 'My agent'
		positionLimit = draft.positionLimit ?? '1000'
		dailyLossLimit = draft.dailyLossLimit ?? '250'
		accessDuration = draft.accessDuration ?? '30'
		agentRecordId = draft.agentRecordId ?? ''
		agentPublicKey = draft.agentPublicKey ?? null
	})

	$effect(() => {
		const accountId = session?.accountId
		if (!accountId || typeof window === 'undefined') return
		const timer = window.setTimeout(() => {
			try { localStorage.setItem(`afterbell.onboarding-draft.v1:${accountId}`, JSON.stringify({ usernameMode, selectedMarkets, leverage, username, reclaimEmail, agentName, positionLimit, dailyLossLimit, accessDuration, agentRecordId, agentPublicKey })) } catch { /* Draft persistence is best-effort. */ }
		}, 400)
		return () => window.clearTimeout(timer)
	})

	function toggleMarket(ticker: string) { selectedMarkets = selectedMarkets.includes(ticker) ? selectedMarkets.filter(item => item !== ticker) : [...selectedMarkets, ticker] }
	async function copyAddress(address: string) { try { await navigator.clipboard.writeText(address); showToast('success', 'Wallet address copied.') } catch { showToast('error', 'Could not copy the wallet address.') } }
	async function disconnect() { await import('../../lib/wallet-core').then(m => m.disconnectWallet()); walletModalOpen = false; await goto('/', { replaceState: true }) }
	async function continueSetup() {
		const current = session
		if (!current || isClaimingUsername) return
		if (usernameMode === 'reclaim' && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(reclaimEmail.trim())) { showToast('error', 'Enter the email address used for your waitlist claim.'); return }
		if (usernameMode === 'new' && !/^[a-z0-9_]{3,20}$/.test(username.trim())) { showToast('error', 'Use 3–20 lowercase letters, numbers, or underscores for your username.'); return }
		isClaimingUsername = true
		const toast = showToast('waiting', usernameMode === 'reclaim' ? 'Looking up your waitlist username…' : 'Checking username availability…', 0)
		try {
			if (usernameMode === 'reclaim') {
				const result = await reclaimAccountUsername({ account_id: current.accountId, email: reclaimEmail.trim() })
				username = result.username; step = 1; dismissToast(toast); showToast('success', `Welcome back, ${result.username}.`)
			} else {
				const availability = await checkAccountUsername(current.accountId, username.trim())
				if (!availability.valid || !availability.available) throw new Error('That username is taken or reserved. Choose another one.')
				const result = await claimAccountUsername({ account_id: current.accountId, username: username.trim() })
				username = result.username; step = 1; dismissToast(toast); showToast('success', result.status === 'already_claimed' ? 'Your username is ready.' : 'Username claimed.')
			}
		} catch (error) { dismissToast(toast); showToast('error', error instanceof Error ? error.message : 'Could not claim that username.') }
		finally { isClaimingUsername = false }
	}
	async function finishSetup() {
		const current = session
		if (!current || isSubmitting) return
		if (!agentName.trim()) { showToast('error', 'Give your agent a name to continue.'); return }
		if (!selectedMarkets.length) { showToast('error', 'Choose at least one market for your agent.'); return }
		isSubmitting = true
		const toast = showToast('waiting', 'Waiting for your wallet to approve the agent permissions…', 0)
		try {
			const recordId = agentRecordId || `agent:${current.accountId}:${crypto.randomUUID().replaceAll('-', '')}`
			const publicKey = agentPublicKey ?? await createAgentKey(current.accountId, recordId)
			agentRecordId = recordId; agentPublicKey = publicKey
			const usdcUnits = (value: string) => (BigInt(value) * 1_000_000n).toString()
			const setup = { account_id: current.accountId, agent_record_id: recordId, agent_key: publicKey, allowed_markets: selectedMarkets, max_position_usdc: usdcUnits(positionLimit), max_open_notional_usdc: usdcUnits(positionLimit), max_leverage_bps: leverage * 10_000, max_allocation_usdc: usdcUnits(positionLimit), max_daily_loss_usdc: usdcUnits(dailyLossLimit), access_duration_days: Number(accessDuration) }
			const challenge = await getAgentSetupPayload(setup)
			const signature = await signWalletMessage(challenge.payload)
			const result = await submitAgentSetup({ ...setup, signature, expires_at_ms: challenge.expires_at_ms, nonce: challenge.nonce })
			localStorage.setItem(`afterbell.onboarding.v1:${current.accountId}`, JSON.stringify({ username: username.trim(), agent: { name: agentName.trim(), markets: selectedMarkets, maxLeverage: leverage, positionLimitUsdc: Number(positionLimit), dailyLossLimitUsdc: Number(dailyLossLimit), accessDurationDays: Number(accessDuration), recordId: result.agent_record_id, transactionHash: result.transaction_hash ?? null } }))
			localStorage.removeItem(`afterbell.onboarding-draft.v1:${current.accountId}`)
			localStorage.removeItem(`afterbell.onboarding-required.v1:${current.accountId}`)
			sessionStorage.removeItem('afterbell.first-time-setup')
			dismissToast(toast); showToast('success', 'Your agent is set up. Welcome to Afterbell.')
			await goto('/trade')
		} catch (error) { dismissToast(toast); showToast('error', error instanceof Error ? error.message : 'Could not create your agent. Please try again.') }
		finally { isSubmitting = false }
	}
</script>

<svelte:head><title>Set up your account | Afterbell</title></svelte:head>

{#if session}
<main class="onboarding-screen">
	<header class="onboarding-header"><a class="onboarding-brand" href="/" aria-label="Afterbell home"><img src="/assets/afterbell-logo.png" alt=""/><span>Afterbell</span></a><button class="onboarding-account" type="button" title="Wallet details" onclick={() => walletModalOpen = true}>{session.address.slice(0, 6)}…{session.address.slice(-4)}</button></header>
	<section class="onboarding-main" aria-label="Account setup">
		<div class="onboarding-intro"><h1>Let’s set things up.</h1><p>Choose how you want to show up, then shape the permissions for your trading agent.</p></div>
		<div class="onboarding-progress" aria-label={`Step ${step + 1} of 2`}><div class="onboarding-progress-track"><span style={`transform:translateX(${step * 100}%)`}></span></div><div class="onboarding-step-labels"><span class:is-current={step === 0} class:is-done={step > 0}><b>{step > 0 ? '✓' : '01'}</b> Username</span><span class:is-current={step === 1}><b>02</b> Agent</span></div></div>
		<div class="onboarding-carousel-viewport"><div class="onboarding-carousel">
			<article class="onboarding-slide" aria-hidden={step !== 0} inert={step !== 0}>
				<div class="onboarding-card-heading"><div class="onboarding-icon-box"><span>@</span></div><div><h2>Choose your username</h2><p>This is how people will find you on Afterbell.</p></div></div>
				<div class="username-mode-switch" role="tablist" aria-label="Username option"><button class:active={usernameMode === 'new'} role="tab" aria-selected={usernameMode === 'new'} onclick={() => usernameMode = 'new'}>Choose a username</button><button class:active={usernameMode === 'reclaim'} role="tab" aria-selected={usernameMode === 'reclaim'} onclick={() => usernameMode = 'reclaim'}>Reclaim waitlist name</button></div>
				{#if usernameMode === 'new'}<div class="username-form-preview"><label for="onboarding-username">Username</label><div class="username-input-wrap"><span>@</span><input bind:value={username} id="onboarding-username" placeholder="yourname" autocomplete="off"/></div><p class="username-availability">3–20 characters: lowercase letters, numbers, and underscores.</p></div>
				{:else}<div class="username-form-preview reclaim-preview"><label for="reclaim-email">Email used for your waitlist claim</label><div class="username-input-wrap"><input bind:value={reclaimEmail} id="reclaim-email" type="email" placeholder="you@example.com" autocomplete="email"/></div><p>We’ll look up the username you reserved with this email.</p></div>{/if}
				<div class="onboarding-note"><span>Your wallet stays in control of your account.</span></div>
			</article>
			<article class="onboarding-slide agent-slide" aria-hidden={step !== 1} inert={step !== 1}>
				<div class="onboarding-card-heading"><div class="onboarding-icon-box"><img class="agent-otto-logo" src="/assets/afterbell-otto-logo.png" alt=""/></div><div><h2>Set up your agent</h2><p>Give it a name and choose what it’s allowed to do.</p></div></div>
				<div class="agent-name-field"><label for="agent-name">Agent name</label><input bind:value={agentName} id="agent-name" placeholder="My trading agent"/></div>
				<div class="agent-markets-field"><div class="agent-field-title"><span>Markets it can trade</span><span>{selectedMarkets.length} selected</span></div><div class="agent-market-options">{#each markets as market (market.ticker)}<button type="button" class:selected={selectedMarkets.includes(market.ticker)} onclick={() => toggleMarket(market.ticker)} aria-pressed={selectedMarkets.includes(market.ticker)}><span>{market.ticker}-USDC</span>{#if selectedMarkets.includes(market.ticker)}<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m5 12 4 4L19 6"/></svg>{/if}</button>{/each}</div></div>
				<div class="agent-limits-grid">
					<label class="agent-limit-control"><span>Maximum leverage</span><strong>{leverage}×</strong><input aria-label="Maximum leverage" type="range" min="1" max="5" bind:value={leverage} /></label>
					<label class="agent-limit-control"><span>Position limit</span><select aria-label="Position limit" bind:value={positionLimit}><option value="250">$250</option><option value="500">$500</option><option value="1000">$1,000</option><option value="2500">$2,500</option><option value="5000">$5,000</option></select></label>
					<label class="agent-limit-control"><span>Daily loss limit</span><select aria-label="Daily loss limit" bind:value={dailyLossLimit}><option value="50">$50</option><option value="100">$100</option><option value="250">$250</option><option value="500">$500</option></select></label>
					<label class="agent-limit-control"><span>Access duration</span><select aria-label="Access duration" bind:value={accessDuration}><option value="1">1 day</option><option value="7">7 days</option><option value="30">30 days</option></select></label>
				</div>
				<div class="onboarding-note"><span>The contract enforces these limits on every agent trade.</span></div>
			</article>
		</div></div>
		<div class="onboarding-controls"><button class="onboarding-back" type="button" onclick={() => step = 0} disabled={step === 0}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M19 12H5m7 7-7-7 7-7"/></svg> Back</button><div class="onboarding-dots" aria-hidden="true"><span class:active={step === 0}></span><span class:active={step === 1}></span></div><button class="onboarding-next" type="button" onclick={step === 0 ? () => void continueSetup() : () => void finishSetup()} disabled={isSubmitting || isClaimingUsername}>{step === 0 ? isClaimingUsername ? 'Checking…' : 'Continue →' : isSubmitting ? 'Setting up…' : 'Finish setup'}</button></div>
		<p class="onboarding-step-footnote">{step === 0 ? 'You can update your profile details later.' : 'Agent permissions can be adjusted whenever you need.'}</p>
	</section>
	{#if walletModalOpen}<WalletDetailsModal {session} onClose={() => walletModalOpen = false} onCopy={copyAddress} onDisconnect={disconnect}/>{/if}
</main>
{/if}
