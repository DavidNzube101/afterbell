<script lang="ts">
	import { onMount } from 'svelte'
	import type { WalletSession } from './wallet-core'
	import { sendNearUsdcDeposit } from './wallet-core'
	import { formatNear, formatUsdc, getDepositInstructions, getNearWalletBalance, usdcToRaw } from './dashboard'
	import { showToast } from './toasts'
	import UsdcIcon from './UsdcIcon.svelte'

	type Page = 'overview' | 'deposit' | 'deposit-wallet' | 'withdraw'
	let { session, balanceRaw, balanceLoaded = false, onClose, onCopy, onWithdraw, onDepositComplete }: {
		session: WalletSession
		balanceRaw: string | null
		balanceLoaded?: boolean
		onClose: () => void
		onCopy: (value: string) => void | Promise<void>
		onWithdraw: (amount: string, receiver: string) => Promise<void>
		onDepositComplete: () => Promise<void>
	} = $props()
	let page = $state<Page>('overview')
	let direction = $state<'next' | 'back'>('next')
	let amount = $state('')
	let depositInput = $state('')
	let withdrawAmount = $state('')
	let withdrawReceiver = $state('')
	let submitting = $state(false)
	let formError = $state('')
	let depositInstructions = $state<{ contract_id: string; token_id: string; network: string } | null>(null)
	let nearBalanceRaw = $state<string | null>(null)
	let nearBalanceLoaded = $state(false)
	let depositResult = $state<{ wallet: string; address: string; transactionHash?: string } | null>(null)
	const nearWallets = [
		{ name: 'HOT Wallet', icon: '/assets/wallet-icons/hot-wallet.jpeg' },
		{ name: 'Nightly Wallet', icon: '/assets/wallet-icons/nightly.jpeg' },
		{ name: 'Meteor Wallet', icon: '/assets/wallet-icons/meteor.png' },
	]
	onMount(() => {
		if (session.chain === 'near') withdrawReceiver = session.address
		void getDepositInstructions(session.accountId).then(value => depositInstructions = value).catch(() => undefined)
		if (session.chain === 'near') void getNearWalletBalance(session.address).then(value => { nearBalanceRaw = value.amount_yocto; nearBalanceLoaded = true }).catch(() => nearBalanceLoaded = true)
		else nearBalanceLoaded = true
	})
	function go(next: Page) { direction = next === 'overview' || (page === 'deposit-wallet' && next === 'deposit') ? 'back' : 'next'; formError = ''; page = next }
	function cleanAmount(value: string) {
		const [whole = '', ...parts] = value.replace(/[^0-9.]/g, '').split('.')
		return `${whole.slice(0, 18)}${parts.length ? `.${parts.join('').slice(0, 6)}` : ''}`
	}
	function submitDeposit(event: SubmitEvent) {
		event.preventDefault()
		try {
			if (!depositInstructions) throw new Error('Deposit details could not be loaded. Try again shortly.')
			const nextAmount = cleanAmount(depositInput)
			depositInput = nextAmount
			if (!/^\d+(\.\d{1,6})?$/.test(nextAmount) || !/[1-9]/.test(nextAmount)) throw new Error('Enter a USDC amount greater than zero, with up to 6 decimals.')
			amount = nextAmount
			go('deposit-wallet')
		} catch (error) { showToast('error', error instanceof Error ? error.message : 'Enter a valid deposit amount.') }
	}
	async function chooseDepositWallet(walletName: string) {
		if (!depositInstructions || submitting) return
		submitting = true; formError = ''; depositResult = null
		try {
			const result = await sendNearUsdcDeposit(walletName, depositInstructions.network, depositInstructions.token_id, depositInstructions.contract_id, session.accountId, usdcToRaw(amount.trim()))
			depositResult = { wallet: walletName, address: result.accountId, transactionHash: result.transactionHash }
			await onDepositComplete()
		} catch (error) { formError = error instanceof Error ? error.message : 'The USDC deposit did not complete.' }
		finally { submitting = false }
	}
	async function submitWithdrawal(event: SubmitEvent) {
		event.preventDefault(); formError = ''
		try {
			const entered = withdrawAmount.trim(), receiver = withdrawReceiver.trim()
			if (!/^\d+(\.\d{1,6})?$/.test(entered) || Number(entered) <= 0) throw new Error('Enter a valid withdrawal amount, up to 6 decimals.')
			if (!receiver) throw new Error('Enter a NEAR account to receive USDC.')
			submitting = true; await onWithdraw(entered, receiver); onClose()
		} catch (error) { formError = error instanceof Error ? error.message : 'Withdrawal failed.' }
		finally { submitting = false }
	}
	function setMax() { withdrawAmount = balanceRaw === null ? '' : formatUsdc(balanceRaw) }
</script>

<div class="balance-modal-backdrop" role="presentation" onmousedown={(event) => { if (event.target === event.currentTarget) onClose() }}>
	<div class="balance-modal" role="dialog" aria-modal="true" aria-labelledby="balance-modal-title">
		<header class="balance-modal-header"><div>{#if page !== 'overview'}<button class="balance-back" onclick={() => go(page === 'deposit-wallet' ? 'deposit' : 'overview')} aria-label="Back">←</button>{/if}<div><h2 id="balance-modal-title">{page === 'overview' ? 'Account balance' : page === 'deposit' || page === 'deposit-wallet' ? 'Deposit USDC' : 'Withdraw USDC'}</h2><p>{page === 'overview' ? 'Your Afterbell account' : page === 'deposit' ? 'Choose an amount to deposit' : page === 'deposit-wallet' ? 'Choose a NEAR wallet to continue' : 'Send USDC to a NEAR account'}</p></div></div><button class="balance-close" aria-label="Close balance modal" onclick={onClose}>×</button></header>
		<div class={`balance-carousel ${direction}`}>
			{#if page === 'overview'}<div class="balance-overview-page"><div class="balance-main-value"><UsdcIcon size={28}/> <div><span>USDC balance</span><strong>{!balanceLoaded ? 'Loading…' : balanceRaw === null ? 'Unavailable' : `$${formatUsdc(balanceRaw)}`}</strong></div></div><div class="balance-sub-value"><span>NEAR wallet balance</span><b>{session.chain === 'near' ? !nearBalanceLoaded ? 'Loading…' : nearBalanceRaw === null ? 'Unavailable' : `${formatNear(nearBalanceRaw)} NEAR` : 'Only shown for NEAR wallets'}</b></div><div class="balance-actions"><button onclick={() => go('deposit')}><span>↓</span><b>Deposit</b></button><button onclick={() => go('withdraw')}><span>↑</span><b>Withdraw</b></button></div><p class="balance-account-id">Afterbell account <b>{session.accountId}</b></p></div>{/if}
			{#if page === 'deposit'}<form class="balance-deposit-page" onsubmit={submitDeposit}><label class="deposit-amount-label">Amount to deposit<div class="deposit-amount-input"><input bind:value={depositInput} inputmode="decimal" maxlength="25" placeholder="0.00" aria-label="USDC deposit amount"/><UsdcIcon size={19}/> <span>USDC</span></div></label><p class="deposit-note">Deposit NEAR USDC into your Afterbell account. You’ll choose a NEAR wallet to approve the transfer.</p><button class="balance-primary-action" disabled={!depositInstructions}>Continue</button></form>{/if}
			{#if page === 'deposit-wallet'}<div class="deposit-wallet-page">{#if depositResult}<div class="deposit-complete" role="status"><span class="deposit-complete-icon">✓</span><h3>Deposit submitted</h3><p>{depositResult.wallet} signed the transfer from <b>{depositResult.address}</b>.</p>{#if depositResult.transactionHash}<button class="deposit-copy-address" onclick={() => void onCopy(depositResult!.transactionHash!)}>{depositResult.transactionHash}<span>Copy tx</span></button>{/if}<p class="deposit-note">Your Afterbell USDC balance will update after the transfer is indexed.</p><button class="balance-primary-action" onclick={() => go('overview')}>Done</button></div>{:else}<div class="deposit-network-heading"><img class="near-network-icon" src="/assets/wallet-icons/chain-icons/near.jpeg" alt="NEAR"/><div><b>NEAR wallets</b><small>Choose a wallet to sign this deposit</small></div></div><div class="deposit-wallet-options">{#each nearWallets as wallet (wallet.name)}<button class="deposit-wallet-option" disabled={submitting} onclick={() => void chooseDepositWallet(wallet.name)}><img src={wallet.icon} alt=""/><span>{wallet.name}</span><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m9 18 6-6-6-6" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg></button>{/each}</div>{#if submitting}<p class="deposit-progress" role="status"><span></span>Waiting for wallet approval and transaction confirmation…</p>{/if}{#if formError}<p class="balance-error" role="alert">{formError}</p>{/if}<p class="deposit-selected-amount"><UsdcIcon size={17}/> <b>{amount} USDC</b><span>to {session.accountId}</span></p><p class="deposit-note">This wallet only approves the deposit. Your Afterbell login wallet stays connected.</p>{/if}</div>{/if}
			{#if page === 'withdraw'}<form class="balance-withdraw-page" onsubmit={submitWithdrawal}><div class="withdraw-available"><span>Available to withdraw</span><b>{!balanceLoaded ? 'Loading…' : balanceRaw === null ? 'Unavailable' : `$${formatUsdc(balanceRaw)}`} USDC</b></div><label>NEAR recipient account<input bind:value={withdrawReceiver} placeholder="name.near" autocomplete="off"/></label><label>Amount<div class="withdraw-amount-input"><input bind:value={withdrawAmount} inputmode="decimal" maxlength="25" placeholder="0.00"/><UsdcIcon size={18}/> <span>USDC</span><button type="button" onclick={setMax}>Max</button></div></label>{#if formError}<p class="balance-error" role="alert">{formError}</p>{/if}<p class="withdraw-note">You’ll sign the withdrawal with your connected wallet.</p><button class="balance-primary-action" disabled={submitting || balanceRaw === null}>{submitting ? 'Submitting…' : 'Review and withdraw'}</button></form>{/if}
		</div>
	</div>
</div>
