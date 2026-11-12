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
  toasts.update(current => [...current, { id, kind, message }])
  if (duration > 0 && typeof window !== 'undefined') window.setTimeout(() => dismissToast(id), duration)
  return id
}
