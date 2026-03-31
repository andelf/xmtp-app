/**
 * Settings store -- persists user preferences via SecureStore.
 *
 * Currently manages:
 *   - Custom quick-reaction list (emoji or short text, max 4 chars each)
 */
import { create } from "zustand";
import * as SecureStore from "expo-secure-store";

const REACTIONS_KEY = "settings_quick_reactions";
const READ_RECEIPTS_KEY = "settings_read_receipts";
const DEFAULT_REACTIONS = ["👍", "❤️", "😂", "🔥", "👀", "🙏"];

export interface SettingsState {
  quickReactions: string[];
  readReceipts: boolean;
  isLoaded: boolean;
}

export interface SettingsActions {
  /** Load settings from SecureStore. Call once on app start. */
  load: () => Promise<void>;
  /** Replace the full quick-reaction list and persist. */
  setQuickReactions: (reactions: string[]) => Promise<void>;
  /** Toggle read receipts on/off and persist. */
  toggleReadReceipts: () => Promise<void>;
}

export type SettingsStore = SettingsState & SettingsActions;

export const useSettingsStore = create<SettingsStore>((set) => ({
  quickReactions: DEFAULT_REACTIONS,
  readReceipts: false,
  isLoaded: false,

  load: async () => {
    try {
      const raw = await SecureStore.getItemAsync(REACTIONS_KEY);
      const rrRaw = await SecureStore.getItemAsync(READ_RECEIPTS_KEY);
      const updates: Partial<SettingsState> = { isLoaded: true };
      if (rrRaw) {
        updates.readReceipts = rrRaw === "true";
      }
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed) && parsed.length > 0) {
          updates.quickReactions = parsed;
        }
      }
      set(updates);
    } catch {
      // ignore parse errors, use defaults
    }
    set({ isLoaded: true });
  },

  toggleReadReceipts: async () => {
    const current = useSettingsStore.getState().readReceipts;
    const next = !current;
    set({ readReceipts: next });
    await SecureStore.setItemAsync(READ_RECEIPTS_KEY, String(next));
  },

  setQuickReactions: async (reactions: string[]) => {
    // Validate: non-empty, each item ≤ 4 chars
    const valid = reactions.filter((r) => r.length > 0 && r.length <= 4);
    if (valid.length === 0) return;
    set({ quickReactions: valid });
    await SecureStore.setItemAsync(REACTIONS_KEY, JSON.stringify(valid));
  },
}));

export { DEFAULT_REACTIONS };
