// stores/useTabStore.ts
import { create } from 'zustand'
interface TabState {

  currentTab: string
  setCurrentTab: (tab: string) => void
}

export const useTabStore = create<TabState>((set) => ({
  currentTab: 'home',    // 默认值
  setCurrentTab: (tab) => set({ currentTab: tab }),
}))