<script lang="ts">
	import type { WalletSession } from './wallet-core'
	let { session, onClose, onCopy, onDisconnect }: { session: WalletSession; onClose: () => void; onCopy: (address: string) => void | Promise<void>; onDisconnect: () => void | Promise<void> } = $props()
	const chainName = $derived(session.chain === 'near' ? 'NEAR' : session.chain === 'evm' ? 'EVM' : 'Solana')
	const chainImage = $derived(session.chain === 'near' ? 'near.jpeg' : session.chain === 'evm' ? 'etheruem.jpeg' : 'solana.jpeg')
	const walletImage = $derived.by(() => session.walletName.toLowerCase().startsWith('hot') ? 'hot-wallet.jpeg' : session.walletName.toLowerCase().startsWith('meteor') ? 'meteor.png' : session.walletName.toLowerCase().startsWith('nightly') ? 'nightly.jpeg' : session.walletName.toLowerCase().startsWith('phantom') ? 'phantom.jpeg' : session.walletName.toLowerCase().startsWith('solflare') ? 'solflare.jpeg' : session.walletName.toLowerCase().startsWith('coinbase') ? 'coinbase-wallet.png' : session.walletName.toLowerCase().startsWith('rabby') ? 'rabby.jpeg' : session.walletName.toLowerCase().startsWith('rainbow') ? 'rainbow.png' : 'metamask.jpeg')
</script>

<div class="onboarding-wallet-backdrop" role="presentation" onmousedown={(event) => { if (event.target === event.currentTarget) onClose() }}>
	<div class="onboarding-wallet-modal" role="dialog" aria-modal="true" aria-label="Wallet details">
		<header><button type="button" class="onboarding-wallet-close" aria-label="Close wallet details" onclick={onClose}>×</button></header>
		<dl>
			<div><dt>Chain</dt><dd class="onboarding-wallet-value"><img src={`/assets/wallet-icons/chain-icons/${chainImage}`} alt=""/>{chainName}</dd></div>
			<div><dt>Wallet</dt><dd class="onboarding-wallet-value"><img src={`/assets/wallet-icons/${walletImage}`} alt=""/>{session.walletName}</dd></div>
			<div><dt>Address</dt><dd class="onboarding-wallet-address">{session.address}</dd></div>
		</dl>
		<footer><button type="button" class="onboarding-wallet-copy" onclick={() => void onCopy(session.address)}>Copy address</button><button type="button" class="onboarding-wallet-disconnect" onclick={() => void onDisconnect()}>Disconnect</button></footer>
	</div>
</div>
