/**
 * Auth store -- manages XMTP client lifecycle and secure key storage.
 */
import { create } from "zustand";
import * as SecureStore from "expo-secure-store";
import type { Client } from "@xmtp/react-native-sdk";
import { initClient, disconnectClient, generateDbEncryptionKey } from "../xmtp/client";
import { clearConversationCache } from "../xmtp/messages";

// ---------------------------------------------------------------------------
// Secure storage keys
// ---------------------------------------------------------------------------

const PRIVATE_KEY_STORE = "xmtp_private_key";
const DB_KEY_STORE = "xmtp_db_encryption_key";
const ENV_STORE = "xmtp_env";
const LOCAL_HOST_STORE = "xmtp_local_host";

/** Encode Uint8Array to hex string for SecureStore (which only stores strings). */
function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** Decode hex string back to Uint8Array. */
function fromHex(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
  }
  return bytes;
}

// ---------------------------------------------------------------------------
// Store types
// ---------------------------------------------------------------------------

export interface AuthState {
  client: Client | null;
  address: string | null;
  inboxId: string | null;
  env: string | null;
  isReady: boolean;
  isLoading: boolean;
  error: string | null;
}

export interface AuthActions {
  /** Initialise from a raw private key. Stores key securely. */
  init: (privateKey: string, env?: string, customLocalHost?: string) => Promise<void>;
  /** Try to restore session from SecureStore on app launch. */
  restore: () => Promise<void>;
  /** Log out -- clear client and stored keys. */
  logout: () => Promise<void>;
}

export type AuthStore = AuthState & AuthActions;

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAuthStore = create<AuthStore>((set, get) => ({
  // State
  client: null,
  address: null,
  inboxId: null,
  env: null,
  isReady: false,
  isLoading: false,
  error: null,

  // Actions
  init: async (privateKey: string, env?: string, customLocalHost?: string) => {
    const resolvedEnv = (env ?? "dev") as "dev" | "production" | "local";
    set({ isLoading: true, error: null });
    try {
      // Retrieve or generate DB encryption key
      let dbKeyHex = await SecureStore.getItemAsync(DB_KEY_STORE);
      let dbKey: Uint8Array;
      if (dbKeyHex) {
        dbKey = fromHex(dbKeyHex);
      } else {
        dbKey = generateDbEncryptionKey();
        dbKeyHex = toHex(dbKey);
        await SecureStore.setItemAsync(DB_KEY_STORE, dbKeyHex);
      }

      const result = await initClient(privateKey, dbKey, resolvedEnv, customLocalHost);

      // Persist private key and env settings securely
      await SecureStore.setItemAsync(PRIVATE_KEY_STORE, privateKey);
      await SecureStore.setItemAsync(ENV_STORE, resolvedEnv);
      if (customLocalHost) {
        await SecureStore.setItemAsync(LOCAL_HOST_STORE, customLocalHost);
      }

      console.log("[XMTP] Client created:", result.address, result.inboxId);

      set({
        client: result.client,
        address: result.address,
        inboxId: result.inboxId,
        env: resolvedEnv,
        isReady: true,
        isLoading: false,
      });
    } catch (err: any) {
      console.error("[XMTP] Init failed:", err);
      set({
        error: err?.message ?? String(err),
        isLoading: false,
      });
    }
  },

  restore: async () => {
    set({ isLoading: true, error: null });
    try {
      const privateKey = await SecureStore.getItemAsync(PRIVATE_KEY_STORE);
      if (!privateKey) {
        set({ isLoading: false });
        return;
      }
      const env = (await SecureStore.getItemAsync(ENV_STORE)) ?? "dev";
      const customLocalHost = (await SecureStore.getItemAsync(LOCAL_HOST_STORE)) ?? undefined;
      // Delegate to init which handles DB key retrieval
      await get().init(privateKey, env, customLocalHost);
    } catch (err: any) {
      console.error("[XMTP] Restore failed:", err);
      set({ error: err?.message ?? String(err), isLoading: false });
    }
  },

  logout: async () => {
    disconnectClient();
    clearConversationCache();
    await SecureStore.deleteItemAsync(PRIVATE_KEY_STORE);
    // Keep DB key so the user can re-login and access history
    // Keep env/customLocalHost settings so the user doesn't have to re-select
    set({
      client: null,
      address: null,
      inboxId: null,
      isReady: false,
      error: null,
    });
  },
}));
