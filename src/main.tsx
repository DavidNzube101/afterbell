import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider, createRouter } from '@tanstack/react-router'
import { routeTree } from './routeTree.gen'
import { WalletSessionProvider } from './lib/wallet-session'
import { ToastProvider } from './lib/toasts'

const router = createRouter({ routeTree })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ToastProvider>
      <WalletSessionProvider>
        <RouterProvider router={router} />
      </WalletSessionProvider>
    </ToastProvider>
  </StrictMode>,
)
