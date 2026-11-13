<script lang="ts">
	import { onMount } from 'svelte'
	import { goto } from '$app/navigation'
	import DashboardHeader from '../../lib/DashboardHeader.svelte'
	import { markets } from '../../lib/market-data'
	import { initializeWalletSession, walletSession } from '../../lib/wallet-core'
	import { showToast } from '../../lib/toasts'

	let marketPair = $state('NVDA-USDC')
	let message = $state('')
	let textarea = $state<HTMLTextAreaElement>()
	const starters = [
		{ icon: '↗', label: 'Break down the latest price action' },
		{ icon: '◷', label: 'Explain funding and open interest' },
		{ icon: '⇄', label: 'Compare this market with another' },
		{ icon: '⌁', label: 'Help me understand my position' },
	]
	const pairs = markets.map(market => `${market.ticker}-USDC`)

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) void goto('/', { replaceState: true })
	})

	function chooseStarter(prompt: string) {
		message = `${prompt} for ${marketPair.replace('-USDC', '')}.`
		requestAnimationFrame(() => { textarea?.focus(); resizeComposer() })
	}
	function resizeComposer() {
		if (!textarea) return
		textarea.style.height = 'auto'
		textarea.style.height = `${Math.min(textarea.scrollHeight, 190)}px`
	}
	function submit() {
		if (!message.trim()) return
		showToast('warning', 'Otto chat is coming soon. Your prompt has not been sent.')
	}
</script>

<svelte:head>
	<title>Otto | Afterbell</title>
	<meta name="description" content="Meet Otto, your Afterbell trading assistant." />
</svelte:head>

{#if $walletSession}
	<div class="trade-app otto-page">
		<DashboardHeader active="Otto" />
		<main class="otto-main">
			<section class="otto-home" aria-labelledby="otto-title">
				<div class="otto-identity"><img src="/assets/afterbell-otto-logo.png" alt="" /><span>OTTO</span></div>
				<h1 id="otto-title">Your markets, in perspective.</h1>
				<p class="otto-subtitle">Ask Otto about markets, positions, and trading concepts.</p>

				<form class="otto-composer" onsubmit={(event) => { event.preventDefault(); submit() }}>
					<label class="otto-sr-only" for="otto-message">Message Otto</label>
					<textarea bind:this={textarea} bind:value={message} oninput={resizeComposer} onkeydown={(event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit() } }} id="otto-message" rows="2" maxlength="1000" placeholder="Ask Otto anything about the markets..."></textarea>
					<div class="otto-composer-bottom">
						<label class="otto-pair-picker" aria-label="Choose market pair">
							<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h9"/><circle cx="18" cy="17" r="2.5"/></svg>
							<span>Market pair</span>
							<select bind:value={marketPair} aria-label="Choose market pair">{#each pairs as pair (pair)}<option value={pair}>{pair}</option>{/each}</select>
							<svg class="otto-chevron" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>
						</label>
						<div class="otto-composer-actions">
							<span class="otto-composer-hint">Otto is getting ready</span>
							<button type="submit" class="otto-send" aria-label="Send message" disabled={!message.trim()}>
								<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6"/></svg>
							</button>
						</div>
					</div>
				</form>

				<div class="otto-starters" aria-label="Starter prompts">
					{#each starters as starter (starter.label)}
						<button type="button" class="otto-starter" onclick={() => chooseStarter(starter.label)}><span aria-hidden="true">{starter.icon}</span>{starter.label}</button>
					{/each}
				</div>
				<p class="otto-disclaimer">Otto is an AI assistant. Review market information before making trading decisions.</p>
			</section>
		</main>
		<footer class="trade-statusbar otto-footer"><span class="status-spacer"></span><a class="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
	</div>
{/if}
