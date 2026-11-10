import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useEffect, useState } from 'react'
import logo from '../assets/afterbell-logo.png'
import { useWalletSession } from '../lib/wallet-session'
import './setting-up.css'

export const Route = createFileRoute('/setting-up')({ component: SettingUpPage })

function SettingUpPage() {
  const navigate = useNavigate()
  const { session } = useWalletSession()
  const [message, setMessage] = useState('Preparing your account')

  useEffect(() => {
    if (!session) {
      void navigate({ to: '/', replace: true })
      return
    }
    let active = true
    const prepare = async () => {
      try {
        const response = await fetch('/api/v1/markets', { headers: { Accept: 'application/json' } })
        if (!response.ok) throw new Error('Afterbell services are not available right now')
        await response.json()
        if (!active) return
        setMessage('Your account is ready')
        await new Promise(resolve => window.setTimeout(resolve, 650))
        if (active) await navigate({ to: '/trade', search: { market: undefined, side: undefined }, replace: true })
      } catch {
        if (active) setMessage('Could not finish setup. Please try again.')
      }
    }
    void prepare()
    return () => { active = false }
  }, [session, navigate])

  return <main className="setup-screen">
    <div className="setup-content">
      <img className="setup-logo" src={logo} alt="Afterbell" />
      <div className="setup-spinner" aria-hidden="true" />
      <h1>Setting up</h1>
      <p>{message}</p>
      {message.startsWith('Could not') && <button onClick={() => window.location.reload()}>Try again</button>}
    </div>
  </main>
}
