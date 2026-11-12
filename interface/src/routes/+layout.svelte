<script lang="ts">
	import { onMount } from 'svelte'
	import { initializeWalletSession } from '../lib/wallet-core'
	import ToastStack from '../lib/ToastStack.svelte'
	import '../lib/styles/global.css'

	onMount(initializeWalletSession)
</script>

<slot />
<ToastStack />
