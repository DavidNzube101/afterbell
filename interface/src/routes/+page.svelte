<svelte:head>
	<title>Afterbell</title>
	<meta name="description" content="Trade cash-settled equity perpetuals on Afterbell." />
</svelte:head>

<main class="migration-shell">
	<div class="migration-card">
		<span class="eyebrow">AFTERBELL</span>
		<h1>SvelteKit migration workspace</h1>
		<p>The app shell is in place. Auth, Trade, Markets, Liquidity, and setup are being ported here before the frontend deployment is switched over.</p>
	</div>
</main>

<style>
	.migration-shell { min-height: 100vh; display: grid; place-items: center; padding: 32px; background: #0c1016; color: #eef2f6; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
	.migration-card { width: min(100%, 520px); padding: 32px; border: 1px solid #202a36; border-radius: 16px; background: #10161f; }
	.eyebrow { color: #65aaff; font-size: 11px; letter-spacing: .16em; }
	h1 { margin: 18px 0 10px; font-size: 26px; }
	p { color: #9ba7b5; line-height: 1.6; }
</style>
