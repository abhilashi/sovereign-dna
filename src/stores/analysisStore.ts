import { create } from 'zustand';
import {
  getHealthRisks,
  getPharmacogenomics,
  getTraits,
  getAncestry,
  getCarrierStatus,
} from '../lib/tauri-bridge';
import type {
  HealthRiskResult,
  PharmaResult,
  TraitResult,
  AncestryResult,
  CarrierResult,
} from '../lib/tauri-bridge';

interface AnalysisState {
  healthRisks: HealthRiskResult[] | null;
  pharmacogenomics: PharmaResult[] | null;
  traits: TraitResult[] | null;
  ancestry: AncestryResult | null;
  carrierStatus: CarrierResult[] | null;
  loading: Record<string, boolean>;
  errors: Record<string, string | null>;
  loadHealthRisks: (genomeId: number) => Promise<void>;
  loadPharmacogenomics: (genomeId: number) => Promise<void>;
  loadTraits: (genomeId: number) => Promise<void>;
  loadAncestry: (genomeId: number) => Promise<void>;
  loadCarrierStatus: (genomeId: number) => Promise<void>;
  clearAll: () => void;
}

export const useAnalysisStore = create<AnalysisState>((set) => ({
  healthRisks: null,
  pharmacogenomics: null,
  traits: null,
  ancestry: null,
  carrierStatus: null,
  loading: {},
  errors: {},

  loadHealthRisks: async (genomeId: number) => {
    set((s) => ({ loading: { ...s.loading, healthRisks: true }, errors: { ...s.errors, healthRisks: null } }));
    try {
      const healthRisks = await getHealthRisks(genomeId);
      set((s) => ({ healthRisks, loading: { ...s.loading, healthRisks: false } }));
    } catch (err) {
      set((s) => ({
        loading: { ...s.loading, healthRisks: false },
        errors: { ...s.errors, healthRisks: String(err) },
      }));
    }
  },

  loadPharmacogenomics: async (genomeId: number) => {
    set((s) => ({ loading: { ...s.loading, pharmacogenomics: true }, errors: { ...s.errors, pharmacogenomics: null } }));
    try {
      const pharmacogenomics = await getPharmacogenomics(genomeId);
      set((s) => ({ pharmacogenomics, loading: { ...s.loading, pharmacogenomics: false } }));
    } catch (err) {
      set((s) => ({
        loading: { ...s.loading, pharmacogenomics: false },
        errors: { ...s.errors, pharmacogenomics: String(err) },
      }));
    }
  },

  loadTraits: async (genomeId: number) => {
    set((s) => ({ loading: { ...s.loading, traits: true }, errors: { ...s.errors, traits: null } }));
    try {
      const traits = await getTraits(genomeId);
      set((s) => ({ traits, loading: { ...s.loading, traits: false } }));
    } catch (err) {
      set((s) => ({
        loading: { ...s.loading, traits: false },
        errors: { ...s.errors, traits: String(err) },
      }));
    }
  },

  loadAncestry: async (genomeId: number) => {
    set((s) => ({ loading: { ...s.loading, ancestry: true }, errors: { ...s.errors, ancestry: null } }));
    try {
      const ancestry = await getAncestry(genomeId);
      set((s) => ({ ancestry, loading: { ...s.loading, ancestry: false } }));
    } catch (err) {
      set((s) => ({
        loading: { ...s.loading, ancestry: false },
        errors: { ...s.errors, ancestry: String(err) },
      }));
    }
  },

  loadCarrierStatus: async (genomeId: number) => {
    set((s) => ({ loading: { ...s.loading, carrierStatus: true }, errors: { ...s.errors, carrierStatus: null } }));
    try {
      const carrierStatus = await getCarrierStatus(genomeId);
      set((s) => ({ carrierStatus, loading: { ...s.loading, carrierStatus: false } }));
    } catch (err) {
      set((s) => ({
        loading: { ...s.loading, carrierStatus: false },
        errors: { ...s.errors, carrierStatus: String(err) },
      }));
    }
  },

  clearAll: () => {
    set({
      healthRisks: null,
      pharmacogenomics: null,
      traits: null,
      ancestry: null,
      carrierStatus: null,
      loading: {},
      errors: {},
    });
  },
}));
