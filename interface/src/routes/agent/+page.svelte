<script lang="ts">
	import { onMount, tick } from 'svelte'
	import { goto } from '$app/navigation'
	import { siNear } from 'simple-icons'
	import DashboardHeader from '../../lib/DashboardHeader.svelte'
	import TickerIcon from '../../lib/TickerIcon.svelte'
	import { markets } from '../../lib/market-data'
	import { initializeWalletSession, walletSession } from '../../lib/wallet-core'
	import { showToast } from '../../lib/toasts'

	type Provider = 'near_ai' | 'qwen' | 'byok'
	type Message = { role: 'user' | 'assistant'; content: string }
	type ProviderStatus = { available: boolean; model?: string }

	let marketPair = $state('NVDA-USDC')
	let message = $state('')
	let textarea = $state<HTMLTextAreaElement>()
	let conversationEl = $state<HTMLDivElement>()
	let starterPage = $state(0)
	let carouselPaused = $state(false)
	let marketPickerOpen = $state(false)
	let providerPickerOpen = $state(false)
	let provider = $state<Provider | null>(null)
	let providerChoice = $state<Provider | null>(null)
	let providers = $state<Record<Provider, ProviderStatus>>({
		near_ai: { available: false }, qwen: { available: false }, byok: { available: true },
	})
	let messages = $state<Message[]>([])
	let sending = $state(false)
	let streamError = $state('')
	let pendingPrompt = $state('')
	let carouselTimer: number | undefined
	let abortController: AbortController | undefined
	let byokUrl = $state('https://api.openai.com/v1')
	let byokModel = $state('gpt-4o-mini')
	let byokKey = $state('')
	const starters = [
		{ icon: '↗', label: 'Break down the latest price action' },
		{ icon: '◷', label: 'Explain funding and open interest' },
		{ icon: '⇄', label: 'Compare this market with another' },
		{ icon: '⌁', label: 'Help me understand my position' },
		{ icon: '⌖', label: 'What are the main risks in this market?' },
		{ icon: '＋', label: 'Explain margin and leverage' },
	]
	const starterPages = [starters.slice(0, 3), starters.slice(3, 6)]
	const selectedTicker = $derived(marketPair.replace('-USDC', ''))
	const accountPreferenceKey = $derived(`afterbell.otto-provider.v1:${$walletSession?.accountId ?? 'guest'}`)
	const providerLabel = $derived(provider === 'near_ai' ? 'NEAR AI' : provider === 'qwen' ? 'Qwen · Groq' : provider === 'byok' ? 'BYOK' : 'Choose provider')

	onMount(() => {
		initializeWalletSession()
		if (!$walletSession) { void goto('/', { replaceState: true }); return }
		try {
			const saved = localStorage.getItem(`afterbell.otto-provider.v1:${$walletSession.accountId}`)
			if (saved === 'near_ai' || saved === 'qwen' || saved === 'byok') provider = saved
		} catch { /* Provider preference is optional. */ }
		void fetch('/api/v1/otto/providers').then(async response => {
			if (!response.ok) return
			const data = await response.json()
			providers = {
				near_ai: data.near_ai ?? { available: false },
				qwen: data.qwen ?? { available: false },
				byok: { available: true },
			}
		}).catch(() => {})
		if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
			carouselTimer = window.setInterval(() => { if (!carouselPaused) starterPage = (starterPage + 1) % starterPages.length }, 4500)
		}
		const onKeydown = (event: KeyboardEvent) => {
			if (event.key === 'Escape') { marketPickerOpen = false; providerPickerOpen = false }
		}
		document.addEventListener('keydown', onKeydown)
		return () => { if (carouselTimer) window.clearInterval(carouselTimer); abortController?.abort(); document.removeEventListener('keydown', onKeydown) }
	})

	function chooseStarter(prompt: string) {
		message = `${prompt} for ${marketPair.replace('-USDC', '')}.`
		requestAnimationFrame(() => { textarea?.focus(); resizeComposer() })
	}
	function preferredChoice(): Provider | null {
		return provider ?? (providers.qwen.available ? 'qwen' : null)
	}
	function resizeComposer() {
		if (!textarea) return
		textarea.style.height = 'auto'
		textarea.style.height = `${Math.min(textarea.scrollHeight, 190)}px`
	}
	function chooseProvider(next: Provider) {
		if (next !== 'byok' && !providers[next].available) return
		if (provider && provider !== next && messages.length) {
			messages = []
			streamError = ''
		}
		if (next !== 'byok') byokKey = ''
		provider = next
		try { localStorage.setItem(accountPreferenceKey, next) } catch { /* Preference storage is optional. */ }
	}
	function openProviderPicker() {
		pendingPrompt = ''
		providerChoice = preferredChoice()
		providerPickerOpen = true
	}
	function submit() {
		const prompt = message.trim()
		if (!prompt || sending) return
		if (!provider || (provider === 'byok' && (!byokKey.trim() || !byokModel.trim() || !byokUrl.trim()))) {
			pendingPrompt = prompt
			providerChoice = preferredChoice()
			providerPickerOpen = true
			return
		}
		void sendPrompt(prompt)
	}
	async function confirmProvider() {
		if (!providerChoice || (providerChoice !== 'byok' && !providers[providerChoice].available)) return
		if (providerChoice === 'byok' && (!byokUrl.trim() || !byokModel.trim() || !byokKey.trim())) {
			showToast('warning', 'Enter the compatible API URL, model, and key to continue.')
			return
		}
		chooseProvider(providerChoice)
		providerPickerOpen = false
		const prompt = pendingPrompt
		pendingPrompt = ''
		if (prompt) await sendPrompt(prompt)
	}
	async function sendPrompt(prompt: string) {
		if (!provider || sending) return
		const nextMessages: Message[] = [...messages, { role: 'user', content: prompt }, { role: 'assistant', content: '' }]
		messages = nextMessages
		message = ''
		resizeComposer()
		sending = true
		streamError = ''
		abortController = new AbortController()
		await tick()
		conversationEl?.scrollTo({ top: conversationEl.scrollHeight, behavior: 'smooth' })
		try {
			const body: Record<string, unknown> = {
				provider,
				account_id: $walletSession?.accountId,
				market_id: selectedTicker,
				messages: nextMessages.slice(0, -1).slice(-16),
			}
			if (provider === 'byok') Object.assign(body, { base_url: byokUrl.trim(), model: byokModel.trim(), api_key: byokKey })
			const response = await fetch('/api/v1/otto/chat', {
				method: 'POST', headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
				body: JSON.stringify(body), signal: abortController.signal,
			})
			if (!response.ok) {
				let detail = 'Otto could not start this response. Please try again.'
				try { const error = await response.json(); detail = error.error?.message ?? detail } catch { /* Keep generic message. */ }
				throw new Error(detail)
			}
			if (!response.body) throw new Error('Streaming is unavailable in this browser. Please reload and try again.')
			const reader = response.body.getReader()
			const decoder = new TextDecoder()
			let buffer = ''
			let finished = false
			while (!finished) {
				const { value, done } = await reader.read()
				buffer += decoder.decode(value, { stream: !done })
				let boundary: number
				while ((boundary = buffer.indexOf('\n\n')) >= 0) {
					const block = buffer.slice(0, boundary)
					buffer = buffer.slice(boundary + 2)
					const event = block.split('\n').find(line => line.startsWith('event:'))?.slice(6).trim()
					const dataLine = block.split('\n').find(line => line.startsWith('data:'))?.slice(5).trim()
					if (!dataLine) continue
					let data: { text?: string; message?: string } = {}
					try { data = JSON.parse(dataLine) } catch { continue }
					if (event === 'delta' && data.text) {
						const last = messages.length - 1
						messages[last] = { ...messages[last], content: messages[last].content + data.text }
						if (conversationEl && conversationEl.scrollHeight - conversationEl.scrollTop - conversationEl.clientHeight < 160) conversationEl.scrollTop = conversationEl.scrollHeight
					} else if (event === 'error') {
						throw new Error(data.message || 'Otto could not complete the response.')
					} else if (event === 'done') finished = true
				}
				if (done) break
			}
			if (!finished) throw new Error('The provider ended the stream early. Please try again.')
		} catch (error) {
			if (error instanceof DOMException && error.name === 'AbortError') return
			streamError = error instanceof Error ? error.message : 'Otto could not complete the response.'
			const last = messages.length - 1
			if (last >= 0 && messages[last].role === 'assistant' && !messages[last].content) messages = messages.slice(0, -1)
			showToast('error', streamError)
		} finally {
			sending = false
			abortController = undefined
		}
	}
	function newConversation() {
		abortController?.abort()
		messages = []
		streamError = ''
		sending = false
		message = ''
		requestAnimationFrame(() => { textarea?.focus(); resizeComposer() })
	}
	function chooseMarket(ticker: string) {
		if (selectedTicker !== ticker && messages.length) newConversation()
		marketPair = `${ticker}-USDC`
		marketPickerOpen = false
	}
</script>

<svelte:head>
	<title>Otto | Afterbell</title>
	<meta name="description" content="Meet Otto, your Afterbell trading assistant." />
</svelte:head>

{#if $walletSession}
	<div class="trade-app otto-page">
		<DashboardHeader active="Otto" />
		<main class:otto-chat-mode={messages.length > 0} class="otto-main">
			<section class="otto-home" aria-labelledby="otto-title">
				<div class="otto-identity"><img src="/assets/afterbell-otto-logo.png" alt="" /><span>Otto</span>
					{#if messages.length}<button class="otto-new-chat" type="button" onclick={newConversation}>New chat</button>{/if}
				</div>
				{#if messages.length === 0}
					<h1 id="otto-title">Your markets, in perspective.</h1>
					<p class="otto-subtitle">Ask Otto about markets, positions, and trading concepts.</p>
				{:else}
					<div class="otto-conversation" bind:this={conversationEl} aria-live="polite" aria-label="Conversation with Otto">
						{#each messages as item, index (`${index}:${item.role}`)}
							<div class:otto-user-message={item.role === 'user'} class:otto-assistant-message={item.role === 'assistant'} class="otto-message">
								{#if item.role === 'assistant'}<img src="/assets/afterbell-otto-logo.png" alt="Otto" />{/if}
								<div class="otto-message-content">{item.content || (sending && index === messages.length - 1 ? 'Thinking…' : '')}</div>
							</div>
						{/each}
					</div>
				{/if}

				<form class="otto-composer" onsubmit={(event) => { event.preventDefault(); submit() }}>
					<label class="otto-sr-only" for="otto-message">Message Otto</label>
					<textarea bind:this={textarea} bind:value={message} oninput={resizeComposer} onkeydown={(event) => { if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit() } }} id="otto-message" rows="2" maxlength="1000" placeholder="Ask Otto anything about the markets..." disabled={sending}></textarea>
					<div class="otto-composer-bottom">
						<div class="otto-composer-left">
							<button type="button" class="otto-pair-picker" aria-label={`Choose market pair, currently ${marketPair}`} aria-haspopup="dialog" onclick={() => marketPickerOpen = true}>
								<TickerIcon ticker={selectedTicker} size={20}/><span>{marketPair}</span>
								<svg class="otto-chevron" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m7 10 5 5 5-5"/></svg>
							</button>
							<button type="button" class="otto-provider-chip" onclick={openProviderPicker} aria-label={`AI provider: ${providerLabel}. Change provider`}>
								{#if provider === 'near_ai'}<svg viewBox="0 0 24 24" aria-hidden="true"><path d={siNear.path}/></svg>{:else if provider === 'qwen'}<img src="/assets/qwen.png" alt="" />{:else}<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M7 10V7a5 5 0 0 1 10 0v3M5 10h14v11H5z"/><circle cx="12" cy="15" r="1"/></svg>{/if}
								<span>{providerLabel}</span><span class="otto-chip-caret">⌄</span>
							</button>
						</div>
						<div class="otto-composer-actions">
							<button type="submit" class="otto-send" aria-label="Send message" disabled={!message.trim() || sending}>
								{#if sending}<span class="otto-spinner"></span>{:else}<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6"/></svg>{/if}
							</button>
						</div>
					</div>
				</form>

				{#if messages.length === 0}
					<div class="otto-starter-carousel" role="region" aria-roledescription="carousel" aria-label="Starter prompts" onpointerenter={() => carouselPaused = true} onpointerleave={() => carouselPaused = false} onfocusin={() => carouselPaused = true} onfocusout={() => carouselPaused = false}>
						<div class="otto-starter-track" style={`transform:translateX(-${starterPage * 100}%)`}>
							{#each starterPages as page, pageIndex (pageIndex)}
								<div class="otto-starters" role="group" aria-label={`Starter prompts ${pageIndex + 1} of ${starterPages.length}`} aria-hidden={starterPage !== pageIndex} inert={starterPage !== pageIndex}>
									{#each page as starter (starter.label)}
										<button type="button" class="otto-starter" onclick={() => chooseStarter(starter.label)}><span aria-hidden="true">{starter.icon}</span>{starter.label}</button>
									{/each}
								</div>
							{/each}
						</div>
					</div>
				{/if}
				<p class="otto-disclaimer">Otto is an AI assistant. Review market information before making trading decisions.</p>
			</section>
		</main>
		<footer class="trade-statusbar otto-footer"><span class="status-spacer"></span><a class="footer-link" href="https://docs.afterbellfx.xyz" target="_blank" rel="noreferrer">Docs</a><span>© Afterbell 2026</span></footer>
	</div>
{/if}

{#if marketPickerOpen}
	<div class="otto-market-modal-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) marketPickerOpen = false }}>
		<dialog open class="otto-market-modal" aria-labelledby="otto-market-title">
			<header><div><h2 id="otto-market-title">Choose market pair</h2><p>Pick a market for Otto to focus on.</p></div><button type="button" class="otto-market-close" aria-label="Close market picker" onclick={() => marketPickerOpen = false}>×</button></header>
			<div class="otto-market-options">
				{#each markets as market (market.ticker)}
						<button type="button" class:selected={marketPair === `${market.ticker}-USDC`} onclick={() => chooseMarket(market.ticker)}>
						<TickerIcon ticker={market.ticker} size={30}/><span><b>{market.ticker}-USDC</b><small>{market.name}</small></span>{#if marketPair === `${market.ticker}-USDC`}<svg class="otto-market-check" viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg>{/if}
					</button>
				{/each}
			</div>
		</dialog>
	</div>
{/if}

{#if providerPickerOpen}
	<div class="otto-market-modal-backdrop otto-provider-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) providerPickerOpen = false }}>
		<dialog open class="otto-market-modal otto-provider-modal" aria-labelledby="otto-provider-title">
			<header><div><h2 id="otto-provider-title">Choose Otto’s AI provider</h2><p>Your choice is saved for this wallet. Changing providers starts a new conversation.</p></div><button type="button" class="otto-market-close" aria-label="Close provider picker" onclick={() => { providerPickerOpen = false; pendingPrompt = '' }}>×</button></header>
			<div class="otto-provider-options">
				<button type="button" class:selected={providerChoice === 'near_ai'} disabled={!providers.near_ai.available} onclick={() => providerChoice = 'near_ai'}>
					<span class="otto-provider-logo near"><svg viewBox="0 0 24 24" aria-hidden="true"><path d={siNear.path}/></svg></span><span class="otto-provider-copy"><b>NEAR AI</b><small>Powered by NEAR · privacy depends on model tier</small></span>{#if providerChoice === 'near_ai'}<span class="otto-provider-check">✓</span>{/if}
				</button>
				<button type="button" class:selected={providerChoice === 'qwen'} disabled={!providers.qwen.available} onclick={() => providerChoice = 'qwen'}>
					<span class="otto-provider-logo qwen"><img src="/assets/qwen.png" alt="" /></span><span class="otto-provider-copy"><b>Qwen <em>Default</em></b><small>Fast answers powered by Groq</small></span>{#if providerChoice === 'qwen'}<span class="otto-provider-check">✓</span>{/if}
				</button>
				<button type="button" class:selected={providerChoice === 'byok'} onclick={() => providerChoice = 'byok'}>
					<span class="otto-provider-logo byok"><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M7 10V7a5 5 0 0 1 10 0v3M5 10h14v11H5z"/><circle cx="12" cy="15" r="1"/></svg></span><span class="otto-provider-copy"><b>BYOK</b><small>Use your OpenAI-compatible API key</small></span>{#if providerChoice === 'byok'}<span class="otto-provider-check">✓</span>{/if}
				</button>
			</div>
			{#if providerChoice === 'byok'}
				<div class="otto-byok-fields">
					<label>API base URL<input bind:value={byokUrl} type="url" placeholder="https://api.openai.com/v1" autocomplete="url" /></label>
					<label>Model<input bind:value={byokModel} placeholder="gpt-4o-mini" autocomplete="off" /></label>
					<label>API key<input bind:value={byokKey} type="password" placeholder="Paste your API key" autocomplete="new-password" /></label>
					<p>Your key stays in this browser session and is sent only for your requests. It is not saved to this device.</p>
				</div>
			{/if}
			<footer><span>{#if providerChoice && providerChoice !== 'byok'}Model: {providers[providerChoice].model || 'configured by Afterbell'}{:else}No API key is needed for platform providers.{/if}</span><button type="button" class="otto-provider-continue" disabled={!providerChoice || (providerChoice !== 'byok' && !providers[providerChoice].available)} onclick={confirmProvider}>{pendingPrompt ? 'Continue and send' : 'Use this provider'}<span aria-hidden="true">→</span></button></footer>
		</dialog>
	</div>
{/if}
