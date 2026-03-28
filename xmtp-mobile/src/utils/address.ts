/**
 * Address formatting utilities.
 */

/**
 * Shorten an Ethereum-style address to "0xABCD...1234" format (first 6 + last 4 chars).
 *
 * If the input is shorter than 10 characters it is returned as-is.
 */
export function shortenAddress(address: string): string {
  if (address.length <= 10) {
    return address;
  }
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
}
