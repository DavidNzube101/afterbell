<script lang="ts">
	import { dismissToast, toasts } from './toasts'
</script>

<div class="toast-stack" aria-live="polite" aria-relevant="additions text">
	{#each $toasts as toast (toast.id)}
		<div class={`app-toast app-toast-${toast.kind}`} role={toast.kind === 'error' ? 'alert' : 'status'}>
			<span class="app-toast-icon">
				{#if toast.kind === 'waiting'}
					<span class="toast-spinner" aria-hidden="true"></span>
				{:else if toast.kind === 'success'}
					<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m4 10 4 4 8-8" /></svg>
				{:else if toast.kind === 'warning'}
					<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3 18 17H2L10 3Z" /><path d="M10 8v4m0 2v.1" /></svg>
				{:else}
					<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" /></svg>
				{/if}
			</span>
			<span class="app-toast-message">{toast.message}</span>
			<button type="button" class="app-toast-dismiss" aria-label="Dismiss notification" onclick={() => dismissToast(toast.id)}>
				<svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" /></svg>
			</button>
		</div>
	{/each}
</div>
