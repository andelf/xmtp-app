import { create } from "zustand";

export interface DraftEntry {
  text: string;
  updatedAt: number;
}

export interface DraftState {
  byConversation: Record<string, DraftEntry>;
}

export interface DraftActions {
  setDraft: (conversationId: string, text: string) => void;
  clearDraft: (conversationId: string) => void;
  clearAllDrafts: () => void;
}

export type DraftStore = DraftState & DraftActions;

export const useDraftStore = create<DraftStore>((set) => ({
  byConversation: {},

  setDraft: (conversationId, text) => {
    set((state) => ({
      byConversation: {
        ...state.byConversation,
        [conversationId]: {
          text,
          updatedAt: Date.now(),
        },
      },
    }));
  },

  clearDraft: (conversationId) => {
    set((state) => {
      if (!state.byConversation[conversationId]) return state;
      const next = { ...state.byConversation };
      delete next[conversationId];
      return { byConversation: next };
    });
  },

  clearAllDrafts: () => {
    set({ byConversation: {} });
  },
}));
