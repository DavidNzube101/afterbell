export type AgentPublicKey = { NearEd25519: { public_key: number[] } }

function openAgentKeyStore(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('afterbell-agent-keys', 1)
    request.onupgradeneeded = () => request.result.createObjectStore('keys')
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Could not open the agent key store.'))
  })
}

export async function createAgentKey(accountId: string, agentRecordId: string): Promise<AgentPublicKey> {
  let keyPair: CryptoKeyPair
  try {
    keyPair = await crypto.subtle.generateKey({ name: 'Ed25519' } as AlgorithmIdentifier, true, ['sign', 'verify']) as CryptoKeyPair
  } catch {
    throw new Error('This browser cannot create an Ed25519 agent key. Update your browser and try again.')
  }

  const publicBytes = new Uint8Array(await crypto.subtle.exportKey('raw', keyPair.publicKey))
  const privateBytes = await crypto.subtle.exportKey('pkcs8', keyPair.privateKey)
  const protectedPrivateKey = await crypto.subtle.importKey(
    'pkcs8', privateBytes, { name: 'Ed25519' } as AlgorithmIdentifier, false, ['sign'],
  )
  const database = await openAgentKeyStore()
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction('keys', 'readwrite')
    transaction.objectStore('keys').put(protectedPrivateKey, `${accountId}:${agentRecordId}`)
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error ?? new Error('Could not save the agent key.'))
    transaction.onabort = () => reject(transaction.error ?? new Error('Could not save the agent key.'))
  }).finally(() => database.close())

  return { NearEd25519: { public_key: Array.from(publicBytes) } }
}
