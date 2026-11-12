import type { AgentPublicKey } from './agent-key'
import type { WalletMessagePayload } from './wallet-core'

type AgentSetupInput = {
  account_id: string
  agent_record_id: string
  agent_key: AgentPublicKey
  allowed_markets: string[]
  max_position_usdc: string
  max_open_notional_usdc: string
  max_leverage_bps: number
  max_allocation_usdc: string
  max_daily_loss_usdc: string
  access_duration_days: number
}

type ApiError = { error?: { message?: string } }

export async function checkAccountUsername(accountId: string, username: string, signal?: AbortSignal) {
  const query = new URLSearchParams({ account_id: accountId, username })
  let response: Response
  try {
    response = await fetch(`/api/v1/account/username/check?${query}`, { headers: { Accept: 'application/json' }, signal })
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') throw error
    throw new Error('Could not check username availability.')
  }
  const result = await response.json().catch(() => ({})) as ApiError & {
    valid?: boolean
    available?: boolean
    reason?: 'available' | 'already_claimed' | 'wallet_has_username' | 'taken' | 'waitlist_reserved'
    existing_username?: string
  }
  if (!response.ok) throw new Error(result.error?.message || `Username check failed (${response.status}).`)
  return {
    valid: result.valid === true,
    available: result.available === true,
    reason: result.reason,
    existingUsername: result.existing_username,
  }
}

async function post<T>(path: string, body: unknown): Promise<T> {
  let response: Response
  try {
    response = await fetch(`/api${path}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify(body),
    })
  } catch {
    throw new Error('Could not reach the Afterbell API. Check your connection and try again.')
  }
  const result = await response.json().catch(() => ({})) as ApiError
  if (!response.ok) throw new Error(result.error?.message || `Afterbell API request failed (${response.status}).`)
  return result as T
}

export function getAgentSetupPayload(input: AgentSetupInput) {
  return post<{ payload: WalletMessagePayload; expires_at_ms: number; nonce: number }>(
    '/v1/account/agent/payload', input,
  )
}

export function claimAccountUsername(input: { account_id: string; username: string }) {
  return post<{ username: string; status: 'claimed' | 'already_claimed'; reclaimed: false }>(
    '/v1/account/username/claim', input,
  )
}

export function reclaimAccountUsername(input: { account_id: string; email: string }) {
  return post<{ username: string; status: 'claimed' | 'already_claimed'; reclaimed: true }>(
    '/v1/account/username/reclaim', input,
  )
}

export function submitAgentSetup(input: AgentSetupInput & {
  signature: string
  expires_at_ms: number
  nonce: number
}) {
  return post<{ account_id: string; agent_record_id: string; status: 'created' | 'already_created'; transaction_hash?: string }>(
    '/v1/account/agent/create', input,
  )
}
