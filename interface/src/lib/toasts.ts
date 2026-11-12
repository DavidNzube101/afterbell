import { writable } from 'svelte/store'

export type ToastKind = 'waiting' | 'success' | 'error'
export type ToastItem = { id: number; kind: ToastKind; message: string }

export const toasts = writable<ToastItem[]>([])
let nextToastId = 1

export function dismissToast(id: number) {
  toasts.update(current => current.filter(toast => toast.id !== id))
}

export function showToast(kind: ToastKind, message: string, duration = 5000) {
  const id = nextToastId++
  const isBroadcastServerError = kind === 'error' && /broadcast_tx_commit/i.test(message) && /server error/i.test(message)
  const toastMessage = isBroadcastServerError && !/please retry/i.test(message) ? `${message} Please retry.` : message
  toasts.update(current => [...current, { id, kind, message: toastMessage }])
  if (duration > 0 && typeof window !== 'undefined') window.setTimeout(() => dismissToast(id), duration)
  return id
}
