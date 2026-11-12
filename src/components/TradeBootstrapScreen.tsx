import logo from '../assets/afterbell-logo.png'
import './TradeBootstrapScreen.css'

export function TradeBootstrapScreen({ error, onRetry }: { error?: string | null; onRetry?: () => void }) {
  return <main className="trade-bootstrap-screen" aria-live="polite">
    <div className="trade-bootstrap-content">
      <img className="trade-bootstrap-logo" src={logo} alt="Afterbell" />
      <span className={`trade-bootstrap-spinner${error ? ' failed' : ''}`} aria-hidden="true" />
      <h1>{error ? 'Could not load Trade' : 'Loading your trading workspace'}</h1>
      <p>{error || 'Preparing live market data and your account details…'}</p>
      {error && onRetry && <button className="trade-bootstrap-retry" onClick={onRetry}>Try again</button>}
    </div>
  </main>
}
