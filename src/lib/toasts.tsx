import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react'
import './toasts.css'

export type ToastKind = 'waiting' | 'success' | 'error'
type ToastItem = { id: number; kind: ToastKind; message: string }
type ToastContextValue = { showToast: (kind: ToastKind, message: string, duration?: number) => number; dismissToast: (id: number) => void }
const ToastContext = createContext<ToastContextValue | null>(null)
let nextToastId = 1

function ToastIcon({ kind }: { kind: ToastKind }) {
  if (kind === 'waiting') return <span className="toast-spinner" aria-hidden="true" />
  if (kind === 'success') return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m4 10 4 4 8-8" /></svg>
  return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" /></svg>
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([])
  const dismissToast = useCallback((id: number) => setToasts(current => current.filter(toast => toast.id !== id)), [])
  const showToast = useCallback((kind: ToastKind, message: string, duration = 5000) => {
    const id = nextToastId++
    setToasts(current => [...current, { id, kind, message }])
    if (duration > 0) window.setTimeout(() => dismissToast(id), duration)
    return id
  }, [dismissToast])
  const value = useMemo(() => ({ showToast, dismissToast }), [showToast, dismissToast])

  return <ToastContext.Provider value={value}>
    {children}
    <div className="toast-stack" aria-live="polite" aria-relevant="additions text">
      {toasts.map(toast => <div key={toast.id} className={`app-toast app-toast-${toast.kind}`} role={toast.kind === 'error' ? 'alert' : 'status'}>
        <span className="app-toast-icon"><ToastIcon kind={toast.kind} /></span>
        <span className="app-toast-message">{toast.message}</span>
        <button type="button" className="app-toast-dismiss" aria-label="Dismiss notification" onClick={() => dismissToast(toast.id)}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m5 5 10 10M15 5 5 15" /></svg>
        </button>
      </div>)}
    </div>
  </ToastContext.Provider>
}

export function useToasts() {
  const context = useContext(ToastContext)
  if (!context) throw new Error('useToasts must be used within ToastProvider')
  return context
}
