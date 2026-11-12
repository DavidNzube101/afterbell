<script lang="ts">
	import { goto } from '$app/navigation'
	import { onMount } from 'svelte'
	import MarketArtwork from '../lib/MarketArtwork.svelte'
	import { connectWallet, initializeWalletSession, walletConnecting, walletSession, type WalletChain } from '../lib/wallet-core'
	import { dismissToast, showToast } from '../lib/toasts'

	type ChainLabel = 'Solana' | 'EVM' | 'NEAR'
	type Direction = 'forward' | 'backward' | null
	const chainDescriptions: Record<ChainLabel, string> = {
		Solana: 'Connect with a Solana wallet',
		EVM: 'Connect with an Ethereum-compatible wallet',
		NEAR: 'Connect with a NEAR wallet',
	}
	const walletCatalog: Record<ChainLabel, { name: string; icon: string }[]> = {
		Solana: [
			{ name: 'Phantom', icon: '/assets/wallet-icons/phantom.jpeg' },
			{ name: 'Solflare', icon: '/assets/wallet-icons/solflare.jpeg' },
			{ name: 'MetaMask', icon: '/assets/wallet-icons/metamask.jpeg' },
			{ name: 'Coinbase Wallet', icon: '/assets/wallet-icons/coinbase-wallet.png' },
			{ name: 'HOT Wallet', icon: '/assets/wallet-icons/hot-wallet.jpeg' },
			{ name: 'Nightly Wallet', icon: '/assets/wallet-icons/nightly.jpeg' },
		],
		EVM: [
			{ name: 'Phantom', icon: '/assets/wallet-icons/phantom.jpeg' },
			{ name: 'MetaMask', icon: '/assets/wallet-icons/metamask.jpeg' },
			{ name: 'Rainbow', icon: '/assets/wallet-icons/rainbow.png' },
			{ name: 'Rabby', icon: '/assets/wallet-icons/rabby.jpeg' },
			{ name: 'Coinbase Wallet', icon: '/assets/wallet-icons/coinbase-wallet.png' },
			{ name: 'HOT Wallet', icon: '/assets/wallet-icons/hot-wallet.jpeg' },
			{ name: 'Nightly Wallet', icon: '/assets/wallet-icons/nightly.jpeg' },
		],
		NEAR: [
			{ name: 'HOT Wallet', icon: '/assets/wallet-icons/hot-wallet.jpeg' },
			{ name: 'Nightly Wallet', icon: '/assets/wallet-icons/nightly.jpeg' },
			{ name: 'Meteor Wallet', icon: '/assets/wallet-icons/meteor.png' },
		],
	}

	let walletModalOpen = $state(false)
	let selectedChain = $state<ChainLabel | null>(null)
	let transitionDirection = $state<Direction>(null)
	let connectionError = $state('')

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) return
		const firstTime = sessionStorage.getItem('afterbell.first-time-setup') === 'true' ||
			localStorage.getItem(`afterbell.onboarding-required.v1:${$walletSession.accountId}`) === 'true'
		void goto(firstTime ? '/setting-up' : '/trade', { replaceState: true })
	})

	function showWallets(chain: ChainLabel) {
		connectionError = ''
		transitionDirection = 'forward'
		selectedChain = chain
	}
	function showNetworks() {
		transitionDirection = 'backward'
		selectedChain = null
	}
	function closeWalletModal() {
		walletModalOpen = false
		selectedChain = null
		transitionDirection = null
		connectionError = ''
	}
	async function continueWithWallet(walletName: string) {
		if (!selectedChain || $walletConnecting) return
		const toast = showToast('waiting', 'Waiting for wallet approval and account setup…', 0)
		connectionError = ''
		try {
			const chain: WalletChain = selectedChain === 'Solana' ? 'solana' : selectedChain === 'EVM' ? 'evm' : 'near'
			const connected = await connectWallet(chain, walletName)
			dismissToast(toast)
			closeWalletModal()
			const firstTime = sessionStorage.getItem('afterbell.first-time-setup') === 'true'
			showToast('success', firstTime ? 'Wallet connected. Your account is ready to set up.' : 'Wallet connected.', 3500)
			await goto(firstTime ? '/setting-up' : '/trade')
		} catch (error) {
			dismissToast(toast)
			connectionError = error instanceof Error ? error.message : 'Could not connect wallet. Please try again.'
			showToast('error', connectionError)
		}
	}
</script>

<svelte:head>
	<title>Afterbell — Trade equity perpetuals</title>
	<meta name="description" content="Connect your wallet to trade cash-settled equity perpetuals on Afterbell." />
</svelte:head>

<svelte:window onkeydown={(event) => { if (event.key === 'Escape' && walletModalOpen) closeWalletModal() }} />

<main class="auth-layout">
	<section class="auth-panel" aria-labelledby="auth-title">
		<a class="brand" href="/" aria-label="Afterbell home">
			<img class="brand-mark" src="/assets/afterbell-logo.png" alt="" />
			<span>Afterbell</span>
		</a>

		<div class="auth-content">
			<h1 id="auth-title">Welcome to Afterbell</h1>
			<p class="auth-subtitle">Connect your wallet to continue.</p>
			<button class="connect-button" type="button" onclick={() => { walletModalOpen = true }}>
				<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4.5 7.5h13a2 2 0 0 1 2 2v9h-13a2 2 0 0 1-2-2v-11a2 2 0 0 1 2-2h11" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/><path d="M19.5 11h-4a2 2 0 0 0 0 4h4v-4Z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><circle cx="15.5" cy="13" r=".7" fill="currentColor"/></svg>
				<span>Connect wallet</span>
			</button>
			<p class="wallet-note">Your wallet stays in your control.</p>
		</div>

		<footer class="auth-footer">© 2026 Afterbell</footer>
	</section>

	<aside class="art-panel" aria-label="Afterbell visual identity">
		<div class="art-glow"></div>
		<MarketArtwork />
	</aside>

	{#if walletModalOpen}
		<div class="wallet-modal-backdrop" role="presentation" onmousedown={(event) => { if (event.target === event.currentTarget) closeWalletModal() }}>
			<div class={`wallet-modal${selectedChain ? ' wallet-modal-wallets' : ''}`} role="dialog" aria-modal="true" aria-labelledby="wallet-modal-title">
				<header class="wallet-modal-header">
					{#if selectedChain}
						<button class="wallet-modal-back" type="button" onclick={showNetworks} aria-label="Back to wallet networks"><svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m12.5 4.5-5.5 5.5 5.5 5.5" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg></button>
					{:else}<span class="wallet-modal-header-spacer"></span>{/if}
					<button class="wallet-modal-close" type="button" onclick={closeWalletModal} aria-label="Close wallet selection"><svg viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /></svg></button>
				</header>
				<div class={`wallet-modal-content${transitionDirection ? ` wallet-modal-content-${transitionDirection}` : ''}`}>
					<h2 id="wallet-modal-title">{selectedChain ? `${selectedChain} wallets` : 'Choose your network'}</h2>
					<p class="wallet-modal-description">{selectedChain ? chainDescriptions[selectedChain] : 'Choose the network you want to connect with.'}</p>
					{#if selectedChain}
						<div class="wallet-options" aria-label={`${selectedChain} wallets`}>
							{#each walletCatalog[selectedChain] as wallet (wallet.name)}
								<button class="wallet-option" type="button" disabled={$walletConnecting} onclick={() => void continueWithWallet(wallet.name)}>
									<img src={wallet.icon} alt="" /><span>{wallet.name}</span><svg class="wallet-option-chevron" viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m7.5 4.5 5.5 5.5-5.5 5.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
								</button>
							{/each}
						</div>
					{:else}
						<div class="chain-options">
							{#each ['Solana', 'EVM', 'NEAR'] as chain (chain)}
								<button class="chain-option" type="button" onclick={() => showWallets(chain as ChainLabel)}>
									<img class="chain-option-icon" src={chain === 'Solana' ? '/assets/wallet-icons/chain-icons/solana.jpeg' : chain === 'EVM' ? '/assets/wallet-icons/chain-icons/etheruem.jpeg' : '/assets/wallet-icons/chain-icons/near.jpeg'} alt="" />
									<span class="chain-option-copy"><strong>{chain}</strong><small>{chainDescriptions[chain as ChainLabel]}</small></span>
									<svg class="wallet-option-chevron" viewBox="0 0 20 20" fill="none" aria-hidden="true"><path d="m7.5 4.5 5.5 5.5-5.5 5.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
								</button>
							{/each}
						</div>
					{/if}
					{#if connectionError}<p class="wallet-error" role="alert">{connectionError}</p>{/if}
				</div>
			</div>
		</div>
	{/if}
</main>
