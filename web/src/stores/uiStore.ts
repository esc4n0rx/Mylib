import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { ResolvedTheme, ThemeMode } from '@/theme/theme';

interface UiState {
  themeMode: ThemeMode;
  resolvedTheme: ResolvedTheme;
  sidebarCollapsed: boolean;
  setThemeMode: (mode: ThemeMode) => void;
  setResolvedTheme: (theme: ResolvedTheme) => void;
  toggleSidebar: () => void;
}

// Only UI/client state lives here. Server state belongs to TanStack Query.
export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      themeMode: 'system',
      resolvedTheme: 'light',
      sidebarCollapsed: false,
      setThemeMode: (themeMode) => set({ themeMode }),
      setResolvedTheme: (resolvedTheme) => set({ resolvedTheme }),
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
    }),
    {
      name: 'mylib.ui',
      // resolvedTheme is derived at runtime; only persist the user's preference.
      partialize: (s) => ({
        themeMode: s.themeMode,
        sidebarCollapsed: s.sidebarCollapsed,
      }),
    },
  ),
);
