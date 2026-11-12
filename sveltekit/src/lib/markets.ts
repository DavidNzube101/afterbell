import { markets as fallbackMarkets, type Market } from './market-data'

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

type MarketApiResponse = {
  network: 'testnet' | 'mainnet'
  contractId: string
  tradingEnabled: boolean
  markets: Array<{
    id: string
    symbol: string
    baseAsset: string
    quoteAsset: string
    sourceChain: string
    sourceChainId: number
    tokenAddress: string
    status: string
    tradingEnabled: boolean
  }>
}

export async function getMarketRegistry(): Promise<MarketRegistry> {
  const response = await fetch('/api/v1/markets', { headers: { Accept: 'application/json' } })
  if (!response.ok) throw new Error(`API request failed (${response.status})`)
  const data = await response.json() as MarketApiResponse
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
