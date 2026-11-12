export type Market = { ticker: string; name: string }

export const markets: Market[] = [
  { ticker: 'NVDA', name: 'NVIDIA' },
  { ticker: 'TSLA', name: 'Tesla' },
  { ticker: 'AAPL', name: 'Apple' },
  { ticker: 'GOOGL', name: 'Alphabet' },
  { ticker: 'META', name: 'Meta' },
] as const
