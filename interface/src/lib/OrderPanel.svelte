<script lang="ts">
	import { showToast } from './toasts'
	import UsdcIcon from './UsdcIcon.svelte'

	type SubmitOrder = (order: { side: 'long' | 'short'; orderType: 'market' | 'limit'; margin: string; notional: string; leverage: number; limitPrice: string }) => Promise<void>
	let { marketId, initialSide = 'long', availableBalance, maxLeverage = 10, entryPrice = null, tradingEnabled = false, submitting = false, estimatedFee = null, onSubmit }: {
		marketId: string
		initialSide?: 'long' | 'short'
		availableBalance: number | null
		maxLeverage?: number
		entryPrice?: number | null
		tradingEnabled?: boolean
		submitting?: boolean
		estimatedFee?: number | null
		onSubmit: SubmitOrder
	} = $props()

	let side = $state<'long' | 'short'>('long')
	let orderType = $state<'market' | 'limit'>('market')
	let leverage = $state(3)
	let margin = $state('100')
	let limitPrice = $state('')
	let notional = $derived(((/^\d+(\.\d{0,6})?$/.test(margin) ? Number(margin) : 0) * leverage).toFixed(2))
	let leverageProgress = $derived(`${((leverage - 1) / Math.max(1, maxLeverage - 1)) * 100}%`)
	$effect(() => { side = initialSide })
	$effect(() => { if (leverage > maxLeverage) leverage = maxLeverage })

	function chooseMax() {
		if (availableBalance === null) return
		margin = availableBalance.toFixed(2)
	}
	function changeLeverage(value: number) {
		leverage = Math.min(maxLeverage, Math.max(1, value))
	}
	async function submit(event: SubmitEvent) {
		event.preventDefault()
		if (!/^\d+(\.\d{1,6})?$/.test(margin) || Number(margin) <= 0) {
			showToast('error', 'Enter a valid margin greater than zero.')
			return
		}
		if (availableBalance !== null && Number(margin) > availableBalance) {
			showToast('error', 'Margin exceeds your available USDC balance.')
			return
		}
		if (orderType === 'limit' && (!/^\d+(\.\d{1,8})?$/.test(limitPrice) || Number(limitPrice) <= 0)) {
			showToast('error', 'Enter a valid limit price.')
			return
		}
		await onSubmit({ side, orderType, margin, notional, leverage, limitPrice })
	}
</script>

<aside class="order-card" aria-label="Place order">
	<div class="order-card-heading"><h1>Place order</h1></div>
	<form onsubmit={submit}>
		<div class="side-switch"><button type="button" class={side === 'long' ? 'long-active' : ''} onclick={() => side = 'long'}><span>↗</span> Long</button><button type="button" class={side === 'short' ? 'short-active' : ''} onclick={() => side = 'short'}><span>↘</span> Short</button></div>
		<div class="order-types" role="group" aria-label="Order type"><button type="button" class:active={orderType === 'market'} onclick={() => orderType = 'market'}>Market</button><button type="button" class:active={orderType === 'limit'} onclick={() => orderType = 'limit'}>Limit</button><button type="button" class="order-info help-trigger" aria-label="Market and limit order help" aria-describedby="tip-order-types">i<span class="trade-tooltip" id="tip-order-types" role="tooltip">Market orders execute promptly at available prices. Limit orders execute only at your limit price or better, and may not fill.</span></button></div>
		{#if orderType === 'limit'}
			<label class="field-label">Limit price<div class="input-wrap"><span>$</span><input bind:value={limitPrice} inputmode="decimal" autocomplete="off" placeholder="Enter limit price" aria-label="Limit price"/><span class="input-unit">USDC</span></div></label>
		{/if}
		<label class="field-label">Order size<div class="input-wrap"><span>$</span><input value={notional} readonly aria-readonly="true" tabindex="-1" aria-label="Order size"/><span class="input-unit"><UsdcIcon size={17}/>USDC</span></div><small class="input-subtext">Notional size (margin × leverage)</small></label>
		<label class="field-label payment-label"><span>Margin</span><span class="available-label">Available <b>{availableBalance === null ? '—' : `$${availableBalance.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`}</b> <button type="button" onclick={chooseMax}>Max</button></span><div class="input-wrap"><span>$</span><input bind:value={margin} inputmode="decimal" autocomplete="off" maxlength="24" aria-label="Margin amount"/><span class="input-unit"><UsdcIcon size={17}/>USDC</span></div></label>
		<div class={`leverage-control ${side}`} style={`--leverage-progress:${leverageProgress}`}><div class="leverage-heading"><span>Leverage</span><strong>{leverage.toFixed(1)}×</strong></div><input type="range" min="1" max={maxLeverage} step="0.5" value={Math.min(leverage, maxLeverage)} oninput={event => changeLeverage(Number(event.currentTarget.value))} aria-label={`Leverage, maximum ${maxLeverage} times`}/><div class="range-labels">{#each [1, 3, 5, 7, 10].filter(value => value <= maxLeverage || value === 1) as value (value)}<span class:reached={leverage >= value}>{value}×</span>{/each}</div></div>
		<div class="order-summary"><div><span>Entry price</span><b>{entryPrice === null ? 'Waiting for live mark' : `$${entryPrice.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`}</b></div><div><span>Position value</span><b>${Number(notional).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</b></div><div><span>Liquidation</span><b>Uses A1 maintenance rules</b></div><div><span>Trading fee</span><b>{estimatedFee === null ? 'Calculated in quote' : `$${estimatedFee.toFixed(2)}`}</b></div></div>
		<button class={`submit-order ${side}`} type="submit" disabled={submitting || !tradingEnabled}>{submitting ? 'Confirming…' : tradingEnabled ? (side === 'long' ? 'Open long' : 'Open short') : 'Market unavailable'} <span>${Number(notional).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}</span></button>
		<p class="order-disclaimer">{marketId}-USDC · Cash settled</p>
	</form>
</aside>
