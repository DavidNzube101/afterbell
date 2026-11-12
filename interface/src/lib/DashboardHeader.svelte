<script lang="ts">
	import { goto } from '$app/navigation'
	import { walletSession, disconnectWallet } from './wallet-core'
	import { showToast } from './toasts'
	import UsdcIcon from './UsdcIcon.svelte'
	import WalletDetailsModal from './WalletDetailsModal.svelte'
	let { active = 'Trade', balance = null, balanceLoaded = false, notificationCount = 0, onBalanceClick = () => {}, onNotificationsClick = () => {} }: {
		active?: 'Trade' | 'Markets' | 'LP'
		balance?: number | null
		balanceLoaded?: boolean
		notificationCount?: number
		onBalanceClick?: () => void
		onNotificationsClick?: () => void
	} = $props()
	let detailsOpen = $state(false)
	let session = $derived($walletSession)
	function money(value: number) { return `$${value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` }
	async function copy(address: string) { try { await navigator.clipboard.writeText(address); showToast('success', 'Wallet address copied.') } catch { showToast('error', 'Could not copy the wallet address.') } }
	async function disconnect() { await disconnectWallet(); await goto('/', { replaceState: true }) }
	function walletImage(name: string) {
		const n = name.toLowerCase()
		return n.startsWith('hot') ? 'hot-wallet.jpeg' : n.startsWith('meteor') ? 'meteor.png' : n.startsWith('nightly') ? 'nightly.jpeg' : n.startsWith('phantom') ? 'phantom.jpeg' : n.startsWith('solflare') ? 'solflare.jpeg' : n.startsWith('coinbase') ? 'coinbase-wallet.png' : n.startsWith('rabby') ? 'rabby.jpeg' : n.startsWith('rainbow') ? 'rainbow.png' : 'metamask.jpeg'
	}
</script>

<header class="trade-header">
	<a href="/trade" class="trade-brand"><img class="trade-logo" src="/assets/afterbell-logo.png" alt=""/><span>Afterbell</span></a>
	<nav class="trade-nav" aria-label="Main navigation"><a class:nav-active={active === 'Trade'} href="/trade">Trade</a><a class:nav-active={active === 'Markets'} href="/markets">Markets</a><a class:nav-active={active === 'LP'} href="/lp">LP</a><a href="/setting-up">AI agent</a></nav>
	<div class="header-right">
		{#if active === 'Trade'}<button class="icon-button header-bell" aria-label="Notifications" onclick={onNotificationsClick}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M18 9a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"/><path d="M10 21h4"/></svg>{#if notificationCount}<span class="notification-count">{notificationCount}</span>{/if}</button>{/if}
		{#if active === 'Trade' || active === 'LP'}<button class="account-balance-widget" onclick={onBalanceClick}><UsdcIcon size={18}/>{#if !balanceLoaded}<span class="balance-widget-skeleton"></span>{:else}{balance === null ? 'Unavailable' : money(balance)}{/if}</button>{/if}
		{#if session}<button class="account-button" title={`${session.walletName} · ${session.chain}`} onclick={() => detailsOpen = true}><img class="account-avatar wallet-chain-avatar connected-wallet-icon" src={`/assets/wallet-icons/${walletImage(session.walletName)}`} alt=""/><span class="account-address">{session.address.slice(0, 6)}...{session.address.slice(-4)}</span></button>{/if}
	</div>
</header>

{#if detailsOpen && session}<WalletDetailsModal {session} onClose={() => detailsOpen = false} onCopy={copy} onDisconnect={disconnect}/>{/if}
