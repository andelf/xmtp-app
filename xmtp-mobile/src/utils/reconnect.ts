/** Shared reconnect constants and backoff helper for stream hooks. */
export const MAX_RECONNECT = 10;
export const BASE_DELAY = 1000;
export const MAX_DELAY = 30000;

export function backoffDelay(retries: number): number {
  return Math.min(BASE_DELAY * Math.pow(2, retries), MAX_DELAY);
}
