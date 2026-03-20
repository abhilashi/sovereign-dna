import { create } from 'zustand';
import { persist } from 'zustand/middleware';

type Theme = 'light';
type ResearchFrequency = 'daily' | 'weekly' | 'monthly' | 'manual';

interface SettingsState {
  theme: Theme;
  researchUpdateFrequency: ResearchFrequency;
  claudeApiKey: string | null;
  setTheme: (theme: Theme) => void;
  setResearchUpdateFrequency: (freq: ResearchFrequency) => void;
  setClaudeApiKey: (key: string | null) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      theme: 'light',
      researchUpdateFrequency: 'weekly',
      claudeApiKey: null,

      setTheme: (theme: Theme) => set({ theme }),
      setResearchUpdateFrequency: (researchUpdateFrequency: ResearchFrequency) =>
        set({ researchUpdateFrequency }),
      setClaudeApiKey: (claudeApiKey: string | null) => set({ claudeApiKey }),
    }),
    {
      name: 'genome-studio-settings',
    },
  ),
);
