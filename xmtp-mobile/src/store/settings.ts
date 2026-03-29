/**
 * Settings store -- persists user preferences via SecureStore.
 *
 * Currently manages:
 *   - Custom quick-reaction list (emoji or short text, max 4 chars each)
 */
import { create } from "zustand";
import * as SecureStore from "expo-secure-store";

const REACTIONS_KEY = "settings_quick_reactions";
const DEFAULT_REACTIONS = ["👍", "❤️", "😂", "🔥", "👀", "🙏"];

export interface SettingsState {
  quickReactions: string[];
  isLoaded: boolean;
}

export interface SettingsActions {
  /** Load settings from SecureStore. Call once on app start. */
  load: () => Promise<void>;
  /** Replace the full quick-reaction list and persist. */
  setQuickReactions: (reactions: string[]) => Promise<void>;
}

export type SettingsStore = SettingsState & SettingsActions;

export const useSettingsStore = create<SettingsStore>((set) => ({
  quickReactions: DEFAULT_REACTIONS,
  isLoaded: false,

  load: async () => {
    try {
      const raw = await SecureStore.getItemAsync(REACTIONS_KEY);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (Array.isArray(parsed) && parsed.length > 0) {
          set({ quickReactions: parsed, isLoaded: true });
          return;
        }
      }
    } catch {
      // ignore parse errors, use defaults
    }
    set({ isLoaded: true });
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
