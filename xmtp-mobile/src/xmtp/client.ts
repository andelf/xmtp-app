/**
 * XMTP Client singleton management.
 *
 * Uses @xmtp/react-native-sdk v5 (MLS-based).
 * The Signer interface requires: getIdentifier, getChainId, getBlockNumber,
 * signerType, signMessage.
 */
import { Client, PublicIdentity } from "@xmtp/react-native-sdk";
import type { Signer } from "@xmtp/react-native-sdk";
import type { ClientOptions } from "@xmtp/react-native-sdk/build/lib/Client";
import { Wallet } from "ethers";
import { CoinbaseActionsCodec, CoinbaseIntentCodec } from "./coinbaseCodecs";

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

let _client: Client | null = null;

/**
 * Convert an ethers v6 Wallet (from a raw private key) into the XMTP Signer
 * interface expected by Client.create().
 */
function walletToSigner(wallet: Wallet): Signer {
  return {
    getIdentifier: async () => new PublicIdentity(wallet.address.toLowerCase(), "ETHEREUM"),
    getChainId: () => undefined,
    getBlockNumber: () => undefined,
    signerType: () => "EOA" as const,
    signMessage: async (message: string) => {
      try {
        console.log("[Signer] signMessage called, message length:", message.length);
        const signature = await wallet.signMessage(message);
        console.log("[Signer] signMessage success, sig length:", signature.length);
        return { signature };
      } catch (err: any) {
        console.error("[Signer] signMessage FAILED:", err?.message ?? err);
        throw err;
      }
    },
  };
}

/**
 * Generate a random 32-byte encryption key for the local XMTP database.
 * In production this should be derived from a user-controlled secret and
 * persisted in secure storage. For now we generate a fresh key each time --
 * the caller (auth store) is responsible for persisting it.
 */
export function generateDbEncryptionKey(): Uint8Array {
  const key = new Uint8Array(32);
  // crypto.getRandomValues is available in React Native (Hermes)
  if (typeof globalThis.crypto?.getRandomValues === "function") {
    globalThis.crypto.getRandomValues(key);
  } else {
    // Fallback -- not cryptographically secure but unblocks development
    for (let i = 0; i < 32; i++) {
      key[i] = Math.floor(Math.random() * 256);
    }
  }
  return key;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export interface InitClientResult {
  client: Client;
  address: string;
  inboxId: string;
}

/**
 * Create (or re-create) the XMTP client from a raw hex private key.
 *
 * @param privateKey  Hex string, with or without 0x prefix
 * @param dbEncryptionKey  32-byte key for local DB encryption
 * @param env  XMTP environment: "dev" | "production" | "local" (default: "dev")
 * @param customLocalHost  Custom host URL when env is "local"
 */
export async function initClient(
  privateKey: string,
  dbEncryptionKey: Uint8Array,
  env: "dev" | "production" | "local" = "dev",
  customLocalHost?: string
): Promise<InitClientResult> {
  // Normalise key
  const pk = privateKey.startsWith("0x") ? privateKey : `0x${privateKey}`;
  const wallet = new Wallet(pk);
  const signer = walletToSigner(wallet);

  const options: ClientOptions = {
    env,
    dbEncryptionKey,
    ...(env === "local" && customLocalHost ? { customLocalHost } : {}),
  };

  const client = await Client.create(signer, {
    ...options,
    codecs: [new CoinbaseActionsCodec(), new CoinbaseIntentCodec()] as any,
  });
  _client = client;

  return {
    client,
    address: wallet.address.toLowerCase(),
    inboxId: client.inboxId,
  };
}

/** Return the current client, or null if not initialised. */
export function getClient(): Client | null {
  return _client;
}

/** Tear down the current client. */
export function disconnectClient(): void {
  _client = null;
}
