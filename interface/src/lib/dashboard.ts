import type { WalletMessagePayload } from './wallet-core'

const root = '/api'
async function request<T>(path: string, init?: RequestInit, timeoutMs = 12_000): Promise<T> {
  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(new DOMException('Request timed out', 'TimeoutError')), timeoutMs)
  try {
  const response = await fetch(`${root}${path}`, { ...init, signal: init?.signal ?? controller.signal, headers: { Accept: 'application/json', ...(init?.body ? { 'Content-Type': 'application/json' } : {}), ...init?.headers } })
  const body = await response.json().catch(() => ({})) as { error?: { message?: string } }
  if (!response.ok) throw new Error(body.error?.message || `Request failed (${response.status})`)
  return body as T
  } catch (error) {
    if (controller.signal.aborted) throw new Error(`Request timed out after ${Math.round(timeoutMs / 1000)} seconds`)
    throw error
  } finally { clearTimeout(timeout) }
}
const json = (body: unknown): RequestInit => ({ method: 'POST', body: JSON.stringify(body) })
const params = (data: Record<string, string>) => new URLSearchParams(data).toString()

export type MarketSnapshot = {
  market_id: string; enabled: boolean; close_only: boolean; mark_price_e8?: string; observed_at_ms?: number
  confidence_bps?: number; circuit_broken?: boolean; open_interest_usdc: string
  funding_rate_bps_8h: number; max_leverage_bps?: number; max_position_usdc?: string
}
export type PricePoint = { price_e8: string; timestamp_ms: number }
export type VolumePoint = { volume_usdg_raw: string; timestamp_ms: number }
export type PositionRecord = {
  position_id: string; market_id: string; is_long: boolean; notional_usdc: string; margin_usdc?: string
  leverage_bps: number; entry_price: string; mark_price?: string; unrealized_pnl_usdc?: string; mark_observed_at_ms?: number
}
export type LimitOrderRecord = { order_id: string; market_id: string; is_long: boolean; notional_usdc: string; leverage_bps: number; limit_price: string; expires_at_ms: number }
export type ActivityRecord = { id: string; event_type: string; title: string; message: string; market_id: string; side: string; amount_usdc: string; transaction_hash: string; created_at_ms: number }
export type NotificationRecord = ActivityRecord
export type LiquidityOverview = { account_id: string; balance_usdc: string; lp_shares: string; total_lp_shares: string; vault_nav_usdc: string; available_liquidity_usdc: string; vault_balance_usdc: string; withdrawal_request: null | { shares: string; min_amount_usdc: string; requested_at_ms: number } }
export type SignedActionPayload = { account_id: string; message: string; nonce: number; expires_at_ms: number; recipient: string; amount_usdc?: string; shares?: string; min_amount_usdc?: string }
export type TradeBootstrapData = {
  accountId: string
  marketId: string
  range: string
  snapshots: MarketSnapshot[]
  volumes?: Record<string, string>
  points?: PricePoint[]
  volumePoints?: VolumePoint[]
  historyStartMs?: number | null
  positions?: PositionRecord[]
  balanceRaw?: string
  depositInstructions?: { contract_id: string; token_id: string; network: string }
  loadedAt: number
}
let tradeBootstrapCache: TradeBootstrapData | null = null
let tradeBootstrapPending: { key: string; promise: Promise<void> } | null = null

export async function getSnapshots(): Promise<MarketSnapshot[]> {
  const data = await request<unknown>('/v1/markets/snapshot', undefined, 5_000)
  if (!Array.isArray(data)) throw new Error('Market snapshot API returned an invalid response')
  return data.filter((item): item is MarketSnapshot =>
    typeof item === 'object' && item !== null &&
    typeof (item as MarketSnapshot).market_id === 'string' &&
    !('error' in item)
  )
}
export const getMarketHistory = (market: string, range = '1d') => request<{ points: PricePoint[]; volume_points: VolumePoint[]; history_start_ms: number | null }>(`/v1/markets/history?${params({ market_id: market, range })}`)
export const getMarketVolumes = () => request<Array<{ market_id: string; volume_usdc_raw: string }>>('/v1/markets/volume')
export const getLiquidityOverview = (account: string) => request<LiquidityOverview>(`/v1/liquidity/overview?${params({ account_id: account })}`)
export const getDepositInstructions = (account: string) => request<{ account_id: string; contract_id: string; token_id: string; network: string }>(`/v1/account/deposit/instructions?${params({ account_id: account })}`)
export const getNearWalletBalance = (address: string) => request<{ amount_yocto: string; locked_yocto: string; network: string }>(`/v1/account/near-balance?${params({ address })}`)
export const getDepositPayload = (account: string, amount: string) => request<SignedActionPayload>('/v1/liquidity/deposit/payload', json({ account_id: account, amount_usdc: amount }))
export const submitDeposit = (body: unknown) => request<{ transaction_hash: string }>('/v1/liquidity/deposit', json(body))
export const getLpWithdrawalPayload = (account: string, shares: string, minAmount: string) => request<SignedActionPayload>('/v1/liquidity/withdrawal/payload', json({ account_id: account, shares, min_amount_usdc: minAmount }))
export const submitLpWithdrawal = (body: unknown) => request<{ transaction_hash: string }>('/v1/liquidity/withdrawal', json(body))
export const completeLpWithdrawal = (account: string) => request<{ transaction_hash: string }>('/v1/liquidity/withdrawal/complete', json({ account_id: account }))
export const getAccountWithdrawalPayload = (account: string, receiver: string, amount: string) => request<{ account_id: string; receiver_id: string; amount_usdc: string; action_nonce: number; expires_at_ms: number; message: string; nonce: string; recipient: string }>(`/v1/account/withdrawal/payload`, json({ account_id: account, receiver_id: receiver, amount_usdc: amount }))
export const submitAccountWithdrawal = (body: unknown) => request<{ transaction_hash: string }>('/v1/account/withdrawal', json(body))
export const getPositions = (account: string) => request<{ positions: PositionRecord[] }>(`/v1/trade/positions?${params({ account_id: account })}`)
export const getLimits = (account: string) => request<{ orders: LimitOrderRecord[] }>(`/v1/trade/limits?${params({ account_id: account })}`)
export const getHistory = (account: string) => request<{ events: ActivityRecord[] }>(`/v1/trade/history?${params({ account_id: account })}`)
export const getNotifications = (account: string) => request<{ notifications: NotificationRecord[] }>(`/v1/notifications?${params({ account_id: account })}`)
export const getAccountBalance = (account: string) => request<{ account_id: string; balance_usdc: string }>(`/v1/account/balance?${params({ account_id: account })}`)

export async function preloadTradeDashboard(accountId: string, marketId = 'NVDA', range = '1h') {
  const key = `${accountId}:${marketId}:${range}`
  if (tradeBootstrapPending?.key === key) return tradeBootstrapPending.promise
  const promise = getSnapshots().then(snapshots => {
    tradeBootstrapCache = {
      accountId,
      marketId,
      range,
      snapshots,
      loadedAt: Date.now(),
    }
  })
  tradeBootstrapPending = { key, promise }
  try {
    await promise
  } finally {
    if (tradeBootstrapPending?.promise === promise) tradeBootstrapPending = null
  }
}

export function getTradeBootstrapData(accountId: string, marketId: string, range: string) {
  const cached = tradeBootstrapCache
  if (!cached || cached.accountId !== accountId || cached.marketId !== marketId || cached.range !== range || Date.now() - cached.loadedAt > 60_000) return null
  return cached
}

export function clearTradeBootstrapData(accountId: string) {
  if (tradeBootstrapCache?.accountId === accountId) tradeBootstrapCache = null
}
export const deleteNotification = (account: string, id: string) => request(`/v1/notifications/${encodeURIComponent(id)}?${params({ account_id: account })}`, { method: 'DELETE' })
export const clearNotifications = (account: string) => request(`/v1/notifications?${params({ account_id: account })}`, { method: 'DELETE' })
export const postTrade = <T>(path: string, body: unknown) => request<T>(path, json(body))

export function toNearNonce(value: string | number): string {
  const bytes = new Uint8Array(32)
  const n = BigInt(value)
  for (let i = 0; i < 8; i++) bytes[31 - i] = Number((n >> BigInt(i * 8)) & 255n)
  let binary = ''
  bytes.forEach(byte => binary += String.fromCharCode(byte))
  return btoa(binary)
}
export const walletPayload = (message: string, nonce: string | number, recipient: string): WalletMessagePayload => ({ message, nonce: toNearNonce(nonce), recipient })
export function usdc(raw?: string | number | null) { return Number(raw ?? 0) / 1_000_000 }
export function dollars(raw?: string | number | null) { return Number(raw ?? 0) / 100_000_000 }
export function priceToRaw(value: number) { return String(Math.round(value * 100_000_000)) }
export function usdcToRaw(value: string) {
  if (!/^\d+(\.\d{0,6})?$/.test(value)) throw new Error('Enter a valid USDC amount with up to 6 decimals.')
  const [whole, fraction = ''] = value.split('.')
  return (BigInt(whole) * 1_000_000n + BigInt((fraction + '000000').slice(0, 6))).toString()
}
export function formatUsdc(raw?: string | null, fractionDigits = 6) {
  if (!raw || !/^\d+$/.test(raw)) return '0.000000'
  const value = BigInt(raw), whole = value / 1_000_000n, fraction = (value % 1_000_000n).toString().padStart(6, '0')
  return `${whole.toLocaleString()}.${fraction.slice(0, fractionDigits).padEnd(fractionDigits, '0')}`
}
export function formatNear(yocto?: string | null) {
  if (!yocto || !/^\d+$/.test(yocto)) return 'Unavailable'
  const value = BigInt(yocto), whole = value / 10n ** 24n, fraction = (value % (10n ** 24n)).toString().padStart(24, '0').slice(0, 6).replace(/0+$/, '')
  return fraction ? `${whole.toLocaleString()}.${fraction}` : whole.toLocaleString()
}
