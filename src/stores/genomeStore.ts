import { create } from 'zustand';
import { importGenome as tauriImportGenome, listGenomes, deleteGenome as tauriDeleteGenome } from '../lib/tauri-bridge';
import type { Genome } from '../lib/tauri-bridge';

interface GenomeState {
  genomes: Genome[];
  activeGenomeId: number | null;
  loading: boolean;
  error: string | null;
  setActiveGenome: (id: number) => void;
  loadGenomes: () => Promise<void>;
  importGenome: (filePath: string) => Promise<void>;
  deleteGenome: (id: number) => Promise<void>;
}

export const useGenomeStore = create<GenomeState>((set, get) => ({
  genomes: [],
  activeGenomeId: null,
  loading: false,
  error: null,

  setActiveGenome: (id: number) => {
    set({ activeGenomeId: id });
  },

  loadGenomes: async () => {
    set({ loading: true, error: null });
    try {
      const genomes = await listGenomes();
      const activeId = get().activeGenomeId;
      set({
        genomes,
        loading: false,
        activeGenomeId: activeId ?? (genomes.length > 0 ? genomes[0].id : null),
      });
    } catch (err) {
      set({ loading: false, error: String(err) });
    }
  },

  importGenome: async (filePath: string) => {
    set({ loading: true, error: null });
    try {
      await tauriImportGenome(filePath);
      await get().loadGenomes();
    } catch (err) {
      set({ loading: false, error: String(err) });
    }
  },

  deleteGenome: async (id: number) => {
    set({ loading: true, error: null });
    try {
      await tauriDeleteGenome(id);
      const state = get();
      if (state.activeGenomeId === id) {
        set({ activeGenomeId: null });
      }
      await state.loadGenomes();
    } catch (err) {
      set({ loading: false, error: String(err) });
    }
  },
}));
