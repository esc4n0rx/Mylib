import { create } from 'zustand';
import type { CreateLibraryRequest, DatabaseConfig } from '@/api';
import type { AdminStepValues, ServerStepValues } from './schema';

interface SetupDraft {
  step: number;
  server: ServerStepValues;
  admin: AdminStepValues | null;
  database: DatabaseConfig;
  libraries: CreateLibraryRequest[];
  setStep: (step: number) => void;
  setServer: (server: ServerStepValues) => void;
  setAdmin: (admin: AdminStepValues) => void;
  setDatabase: (database: DatabaseConfig) => void;
  addLibrary: (library: CreateLibraryRequest) => void;
  removeLibrary: (index: number) => void;
  reset: () => void;
}

const initialServer: ServerStepValues = {
  serverName: '',
  serverLanguage: 'pt-BR',
};

// Deliberately NOT persisted: the admin password must never touch localStorage.
// A page reload restarts the wizard from clean state.
export const useSetupStore = create<SetupDraft>((set) => ({
  step: 0,
  server: initialServer,
  admin: null,
  database: { type: 'sqlite' },
  libraries: [],
  setStep: (step) => set({ step }),
  setServer: (server) => set({ server }),
  setAdmin: (admin) => set({ admin }),
  setDatabase: (database) => set({ database }),
  addLibrary: (library) => set((s) => ({ libraries: [...s.libraries, library] })),
  removeLibrary: (index) =>
    set((s) => ({ libraries: s.libraries.filter((_, i) => i !== index) })),
  reset: () =>
    set({
      step: 0,
      server: initialServer,
      admin: null,
      database: { type: 'sqlite' },
      libraries: [],
    }),
}));
