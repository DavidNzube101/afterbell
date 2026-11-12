<script lang="ts">
	import { onMount } from 'svelte'
	import { initializeWalletSession } from '../lib/wallet-core'
	import ToastStack from '../lib/ToastStack.svelte'
	import '../lib/styles/global.css'

	onMount(initializeWalletSession)
</script>

<svelte:head><link rel="icon" type="image/png" href="/favicon.png" /></svelte:head>

<slot />
<ToastStack />
