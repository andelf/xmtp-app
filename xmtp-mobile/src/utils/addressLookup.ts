/**
 * Cached inboxId → Ethereum address resolver.
 *
 * Uses client.inboxStates() to batch-resolve inbox IDs to their
 * primary Ethereum address. Results are cached in-memory for the
 * lifetime of the app.
 */
import { getClient } from "../xmtp/client";

const cache = new Map<string, string>();
const pending = new Map<string, Promise<string>>();

/**
 * Resolve a single inboxId to its primary Ethereum address.
 * Returns the cached value immediately if available, otherwise
 * fetches from the network. Falls back to the inboxId on error.
 */
export async function resolveAddress(inboxId: string): Promise<string> {
  const cached = cache.get(inboxId);
  if (cached) return cached;

  // Deduplicate concurrent requests for the same inboxId
  const inflight = pending.get(inboxId);
  if (inflight) return inflight;

  const promise = (async () => {
    try {
      const client = getClient();
      if (!client) return inboxId;

      const states = await client.inboxStates(false, [inboxId as any]);
      if (states.length > 0) {
        const eth = states[0].identities.find((id) => id.kind === "ETHEREUM");
        const address = eth?.identifier ?? inboxId;
        cache.set(inboxId, address);
        return address;
      }
    } catch (err) {
      console.warn("[addressLookup] failed for", inboxId, err);
    } finally {
      pending.delete(inboxId);
    }
    return inboxId;
  })();

  pending.set(inboxId, promise);
  return promise;
}

/**
 * Batch-resolve multiple inboxIds. Skips already-cached entries.
 * Populates the cache for future lookups.
 */
export async function resolveAddresses(inboxIds: string[]): Promise<void> {
  const uncached = inboxIds.filter((id) => !cache.has(id));
  if (uncached.length === 0) return;

  try {
    const client = getClient();
    if (!client) return;

    const states = await client.inboxStates(false, uncached as any[]);
    for (const state of states) {
      const eth = state.identities.find((id) => id.kind === "ETHEREUM");
      if (eth) {
        cache.set(state.inboxId, eth.identifier);
      }
    }
  } catch (err) {
    console.warn("[addressLookup] batch failed:", err);
  }
}

/** Get cached address for an inboxId, or null if not yet resolved. */
export function getCachedAddress(inboxId: string): string | null {
  return cache.get(inboxId) ?? null;
}
