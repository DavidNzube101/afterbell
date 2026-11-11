import { useEffect, useState } from 'react'
import { markets as fallbackMarkets, type Market } from '../data/markets'

export type MarketRegistry = {
  network: 'testnet' | 'mainnet'
  contractId: string
  tradingEnabled: boolean
  markets: Array<Market & {
    tokenAddress: string
    sourceChain: string
    sourceChainId: number
    status: string
    tradingEnabled: boolean
  }>
}

type MarketApiRecord = {
  id: string
  symbol: string
  baseAsset: string
  quoteAsset: string
  sourceChain: string
  sourceChainId: number
  tokenAddress: string
  status: string
  tradingEnabled: boolean
}

type MarketApiResponse = {
  network: 'testnet' | 'mainnet'
  contractId: string
  tradingEnabled: boolean
  markets: MarketApiRecord[]
}

export type ApiMarket = Market & Omit<MarketApiRecord, 'id' | 'symbol' | 'baseAsset' | 'quoteAsset'>

export type ApiStatus = {
  status: 'ready' | 'degraded'
  api: 'running'
  database: 'ready' | 'unavailable'
  indexer: {
    enabled: boolean
    status: 'disabled' | 'starting' | 'running' | 'retrying' | 'stopped'
    lastIndexedBlock: number
    lastSuccessfulPollAtMs?: number
  }
}

const API_PREFIX = '/api'

async function apiGet<T>(path: string): Promise<T> {
  const response = await fetch(`${API_PREFIX}${path}`, { headers: { Accept: 'application/json' } })
  if (!response.ok) throw new Error(`API request failed (${response.status})`)
  return response.json() as Promise<T>
}

export async function getMarketRegistry(): Promise<MarketRegistry> {
  const data = await apiGet<MarketApiResponse>('/v1/markets')
  return {
    network: data.network,
    contractId: data.contractId,
    tradingEnabled: data.tradingEnabled,
    markets: data.markets.map(item => ({
      ticker: item.symbol,
      name: fallbackMarkets.find(market => market.ticker === item.symbol)?.name ?? item.baseAsset,
      tokenAddress: item.tokenAddress,
      sourceChain: item.sourceChain,
      sourceChainId: item.sourceChainId,
      status: item.status,
      tradingEnabled: item.tradingEnabled,
    })),
  }
}

export async function getApiStatus(): Promise<ApiStatus> {
  return apiGet<ApiStatus>('/health/status')
}

export function useMarketRegistry() {
  const [registry, setRegistry] = useState<MarketRegistry | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    void getMarketRegistry().then(data => {
      if (!active) return
      setRegistry(data)
      setError(null)
    }).catch(reason => {
      if (active) setError(reason instanceof Error ? reason.message : 'Could not load markets')
    })
    return () => { active = false }
  }, [])

  return {
    markets: (registry?.markets.length ? registry.markets : fallbackMarkets) as Array<Market & { tradingEnabled?: boolean }>,
    registry,
    error,
    isLive: registry !== null,
  }
}
