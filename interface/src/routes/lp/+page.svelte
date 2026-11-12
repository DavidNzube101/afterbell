<script lang="ts">
	import { onMount } from 'svelte'
	import { goto } from '$app/navigation'
	import DashboardHeader from '../../lib/DashboardHeader.svelte'
	import AccountBalanceModal from '../../lib/AccountBalanceModal.svelte'
	import UsdcIcon from '../../lib/UsdcIcon.svelte'
	import { initializeWalletSession, walletSession, signWalletMessage } from '../../lib/wallet-core'
	import { dismissToast, showToast } from '../../lib/toasts'
	import { formatUsdc, getAccountWithdrawalPayload, getDepositPayload, getLiquidityOverview, getLpWithdrawalPayload, submitAccountWithdrawal, submitDeposit, submitLpWithdrawal, usdcToRaw, walletPayload, type LiquidityOverview } from '../../lib/dashboard'

	let mode = $state<'add' | 'remove'>('add')
	let amount = $state('')
	let minimumOutput = $state('')
	let overview = $state<LiquidityOverview | null>(null)
	let loading = $state(true)
	let submitting = $state(false)
	let balanceModalOpen = $state(false)
	let timer: number | undefined
	let balanceRaw = $derived(overview?.balance_usdc ?? null)
	let balance = $derived(balanceRaw === null ? null : Number(balanceRaw) / 1_000_000)
	let estimatedShares = $derived.by(() => {
		if (!overview || !amount || !/^\d+(\.\d{0,6})?$/.test(amount)) return '0.000000'
		try { const raw = BigInt(usdcToRaw(amount)), nav = BigInt(overview.vault_nav_usdc || '0'), shares = BigInt(overview.total_lp_shares || '0'); return formatUsdc(shares === 0n || nav === 0n ? raw.toString() : (raw * shares / nav).toString()) } catch { return '0.000000' }
	})
	let estimatedUsdcRaw = $derived.by(() => {
		if (!overview || !amount || !/^\d+(\.\d{0,6})?$/.test(amount)) return '0'
		try { const shares = BigInt(usdcToRaw(amount)), total = BigInt(overview.total_lp_shares || '0'), nav = BigInt(overview.vault_nav_usdc || '0'); return total === 0n ? '0' : (shares * nav / total).toString() } catch { return '0' }
	})
	let estimatedMinimum = $derived.by(() => {
		try { const estimate = BigInt(estimatedUsdcRaw), min = estimate * 99n / 100n; return `${min / 1_000_000n}.${(min % 1_000_000n).toString().padStart(6, '0')}` } catch { return '' }
	})

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) { void goto('/', { replaceState: true }); return }
		void refresh()
		timer = window.setInterval(() => { if (document.visibilityState === 'visible') void refresh(false) }, 30_000)
		const onVisibility = () => { if (document.visibilityState === 'visible') void refresh(false) }
		document.addEventListener('visibilitychange', onVisibility)
		return () => { if (timer) window.clearInterval(timer); document.removeEventListener('visibilitychange', onVisibility) }
	})

	async function refresh(showError = true) {
		const session = $walletSession
		if (!session) { overview = null; loading = false; return }
		try { overview = await getLiquidityOverview(session.accountId) }
		catch (error) { if (showError) showToast('error', error instanceof Error ? error.message : 'Could not load vault data.') }
		finally { loading = false }
	}
	function selectMode(value: 'add' | 'remove') { mode = value; amount = ''; minimumOutput = '' }
	async function addLiquidity() {
		const session = $walletSession
		if (!session) { showToast('error', 'Connect your wallet first.'); return }
		try {
			if (!/^\d+(\.\d{1,6})?$/.test(amount)) throw new Error('Enter a valid amount, up to 6 decimals.')
			const raw = usdcToRaw(amount)
			if (BigInt(raw) <= 0n) throw new Error('Enter an amount greater than zero.')
			if (BigInt(raw) > BigInt(overview?.balance_usdc ?? '0')) throw new Error('Your available Afterbell USDC balance is too low.')
			submitting = true
			const toast = showToast('waiting', 'Approve the liquidity action in your wallet.', 0)
			const payload = await getDepositPayload(session.accountId, raw)
			const signature = await signWalletMessage(walletPayload(payload.message, payload.nonce, payload.recipient))
			const result = await submitDeposit({ ...payload, signature })
			dismissToast(toast)
			showToast('success', `Liquidity added. Transaction ${result.transaction_hash.slice(0, 10)}…`)
			amount = ''
			await refresh(false)
		} catch (error) { showToast('error', error instanceof Error ? error.message : 'Could not add liquidity.') }
		finally { submitting = false }
	}
	async function requestWithdrawal() {
		const session = $walletSession
		if (!session) { showToast('error', 'Connect your wallet first.'); return }
		try {
			if (!/^\d+(\.\d{1,6})?$/.test(amount)) throw new Error('Enter a valid LP share amount, up to 6 decimals.')
			if (minimumOutput && !/^\d+(\.\d{1,6})?$/.test(minimumOutput)) throw new Error('Enter a valid minimum USDC amount, up to 6 decimals.')
			const shares = usdcToRaw(amount), min = usdcToRaw(minimumOutput || '0')
			if (BigInt(shares) <= 0n) throw new Error('Enter the LP shares to withdraw.')
			if (BigInt(shares) > BigInt(overview?.lp_shares ?? '0')) throw new Error('You do not have enough LP shares.')
			submitting = true
			const toast = showToast('waiting', 'Approve the withdrawal in your wallet.', 0)
			const payload = await getLpWithdrawalPayload(session.accountId, shares, min)
			const signature = await signWalletMessage(walletPayload(payload.message, payload.nonce, payload.recipient))
			const result = await submitLpWithdrawal({ ...payload, signature })
			dismissToast(toast)
			showToast('success', `Liquidity withdrawn. Transaction ${result.transaction_hash.slice(0, 10)}…`)
			amount = ''
			await refresh(false)
		} catch (error) { showToast('error', error instanceof Error ? error.message : 'Could not withdraw liquidity.') }
		finally { submitting = false }
	}
	async function withdrawFromAccount(amount: string, receiver: string) {
		const session = $walletSession
		if (!session) throw new Error('Connect a wallet before withdrawing.')
		const payload = await getAccountWithdrawalPayload(session.accountId, receiver, usdcToRaw(amount))
		const signature = await signWalletMessage(walletPayload(payload.message, payload.nonce, payload.recipient))
		const result = await submitAccountWithdrawal({ ...payload, signature })
		showToast('success', `Withdrawal submitted. Transaction ${result.transaction_hash.slice(0, 8)}…`)
		await refresh(false)
	}
</script>

<svelte:head><title>Liquidity | Afterbell</title><meta name="description" content="Provide liquidity to the Afterbell shared vault." /></svelte:head>

{#if $walletSession}
<div class="trade-app lp-page">
	<DashboardHeader active="LP" {balance} balanceLoaded={!loading} onBalanceClick={() => balanceModalOpen = true} />
	<main class="lp-content">
		<section class="lp-heading"><div><h1>Provide liquidity. Share the risk.</h1><p>Deposit USDC from your Afterbell account into the shared vault and receive LP shares. Share value follows the vault’s net asset value.</p></div></section>
		<section class="lp-stats" aria-label="Vault overview"><article><span>Vault NAV</span><strong>{#if loading}<i class="metric-skeleton"></i>{:else}${formatUsdc(overview?.vault_nav_usdc)}{/if}</strong><small>USDC</small></article><article><span>Available liquidity</span><strong>{#if loading}<i class="metric-skeleton"></i>{:else}${formatUsdc(overview?.available_liquidity_usdc)}{/if}</strong><small>USDC</small></article><article><span>Your LP shares</span><strong>{#if loading}<i class="metric-skeleton"></i>{:else}{formatUsdc(overview?.lp_shares)}{/if}</strong><small>Share balance</small></article></section>
		<section class="lp-workspace">
			<article class="lp-position-card"><div class="lp-section-heading"><div><h2>Your liquidity</h2><p>Your free balance and current LP share value.</p></div></div>
				{#if loading}<div class="lp-empty-state"><i class="metric-skeleton"></i></div>
				{:else}<div class="lp-account-summary"><div><span>Available Afterbell balance</span><strong><UsdcIcon size={17}/>  {formatUsdc(overview?.balance_usdc)} USDC</strong></div><div><span>Current share value</span><strong>$ {overview && BigInt(overview.total_lp_shares || '0') > 0n ? formatUsdc((BigInt(overview.lp_shares) * BigInt(overview.vault_nav_usdc) / BigInt(overview.total_lp_shares)).toString()) : '0.000000'} USDC</strong></div></div><div class="lp-info-note"><span>i</span><p>Withdrawals return USDC immediately when current vault pricing, your minimum output, and available liquidity checks pass.</p></div>
				{/if}
			</article>
			<article class="lp-action-card"><div class="lp-mode-switch" role="tablist" aria-label="Liquidity action"><button class:active={mode === 'add'} role="tab" aria-selected={mode === 'add'} onclick={() => selectMode('add')}>Add liquidity</button><button class:active={mode === 'remove'} role="tab" aria-selected={mode === 'remove'} onclick={() => selectMode('remove')}>Remove liquidity</button></div>
				{#if mode === 'add'}<div class="lp-form-heading"><h2>Add liquidity</h2><p>Move USDC from your Afterbell balance to the shared vault.</p></div><label class="lp-field-label" for="lp-add-amount">Amount</label><div class="lp-amount-input"><input id="lp-add-amount" bind:value={amount} inputmode="decimal" maxlength="25" placeholder="0.00"/><span class="lp-token"><UsdcIcon size={17}/> USDC</span></div><div class="lp-balance-line"><span>Available balance</span><b>{loading ? '…' : `${formatUsdc(overview?.balance_usdc)} USDC`}</b></div><div class="lp-estimate"><span>Estimated LP shares</span><b>{estimatedShares}</b></div><button class="lp-submit" disabled={loading || submitting} onclick={() => void addLiquidity()}>{submitting ? 'Waiting for wallet…' : 'Add liquidity'}</button>
				{:else}<div class="lp-form-heading"><h2>Remove liquidity</h2><p>Return USDC to your Afterbell balance when vault checks pass.</p></div><label class="lp-field-label" for="lp-remove-amount">Shares to remove</label><div class="lp-amount-input"><input id="lp-remove-amount" bind:value={amount} inputmode="decimal" maxlength="25" placeholder="0.00"/><span class="lp-token">LP shares</span></div><div class="lp-balance-line"><span>Shares available</span><b>{loading ? '…' : formatUsdc(overview?.lp_shares)}</b></div><div class="lp-estimate"><span>Estimated USDC</span><b>{formatUsdc(estimatedUsdcRaw)}</b></div><label class="lp-field-label lp-min-label" for="lp-min-output">Minimum USDC to receive</label><div class="lp-amount-input lp-min-input"><input id="lp-min-output" bind:value={minimumOutput} inputmode="decimal" maxlength="25" placeholder={estimatedMinimum}/><span class="lp-token"><UsdcIcon size={17}/> USDC</span></div><button class="lp-submit remove" disabled={loading || submitting} onclick={() => void requestWithdrawal()}>{submitting ? 'Waiting for wallet…' : 'Withdraw liquidity'}</button>{/if}
			</article>
		</section>
	</main>
	<footer class="trade-statusbar"><span class="status-spacer"></span><a class="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
	{#if balanceModalOpen && $walletSession}<AccountBalanceModal session={$walletSession} {balanceRaw} balanceLoaded={!loading} onClose={() => balanceModalOpen = false} onCopy={async value => { try { await navigator.clipboard.writeText(value); showToast('success', 'Copied to clipboard.') } catch { showToast('error', 'Could not copy to clipboard.') } }} onWithdraw={withdrawFromAccount} onDepositComplete={() => refresh(false)}/>{/if}
</div>
{/if}
