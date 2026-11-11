import type { WalletMessagePayload } from './wallet-session'

const root = '/api'
async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${root}${path}`, { ...init, headers: { Accept: 'application/json', ...(init?.body ? { 'Content-Type': 'application/json' } : {}), ...init?.headers } })
  const body = await response.json().catch(() => ({})) as { error?: { message?: string } }
  if (!response.ok) throw new Error(body.error?.message || `Request failed (${response.status})`)
  return body as T
}
const json = (body: unknown): RequestInit => ({ method: 'POST', body: JSON.stringify(body) })
const params = (data: Record<string, string>) => new URLSearchParams(data).toString()

export type MarketSnapshot = {
  market_id: string; enabled: boolean; close_only: boolean; mark_price_e8?: string; observed_at_ms?: number
  confidence_bps?: number; circuit_broken?: boolean; open_interest_usdc: string
  funding_rate_bps_8h: number; max_leverage_bps?: number; max_position_usdc?: string
}
export type PricePoint = { price_e8: string; timestamp_ms: number }
export type PositionRecord = {
  position_id: string; market_id: string; is_long: boolean; notional_usdc: string; margin_usdc?: string
  leverage_bps: number; entry_price: string; mark_price?: string; unrealized_pnl_usdc?: string; mark_observed_at_ms?: number
}
export type LimitOrderRecord = { order_id: string; market_id: string; is_long: boolean; notional_usdc: string; leverage_bps: number; limit_price: string; expires_at_ms: number }
export type ActivityRecord = { id: string; event_type: string; title: string; message: string; market_id: string; side: string; amount_usdc: string; transaction_hash: string; created_at_ms: number }
export type NotificationRecord = ActivityRecord

export const getSnapshots = () => request<MarketSnapshot[]>('/v1/markets/snapshot')
export const getMarketHistory = (market: string) => request<{ points: PricePoint[] }>(`/v1/markets/history?${params({ market_id: market, range: '1d' })}`)
export const getMarketVolumes = () => request<Array<{ market_id: string; volume_usdc_raw: string }>>('/v1/markets/volume')
export const getPositions = (account: string) => request<{ positions: PositionRecord[] }>(`/v1/trade/positions?${params({ account_id: account })}`)
export const getLimits = (account: string) => request<{ orders: LimitOrderRecord[] }>(`/v1/trade/limits?${params({ account_id: account })}`)
export const getHistory = (account: string) => request<{ events: ActivityRecord[] }>(`/v1/trade/history?${params({ account_id: account })}`)
export const getNotifications = (account: string) => request<{ notifications: NotificationRecord[] }>(`/v1/notifications?${params({ account_id: account })}`)
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
