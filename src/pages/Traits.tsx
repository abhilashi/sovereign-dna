import { useEffect, useState, useMemo } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useAnalysisStore } from '../stores/analysisStore';
import { TRAIT_CATEGORIES } from '../lib/constants';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

export default function Traits() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const { traits, loading, loadTraits } = useAnalysisStore();
  const [selectedCategory, setSelectedCategory] = useState<string>('All');

  useEffect(() => {
    if (activeGenomeId) {
      loadTraits(activeGenomeId);
    }
  }, [activeGenomeId, loadTraits]);

  const filtered = useMemo(() => {
    if (!traits) return [];
    if (selectedCategory === 'All') return traits;
    return traits.filter((t) => t.category === selectedCategory);
  }, [traits, selectedCategory]);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to view traits.</p>
      </div>
    );
  }

  if (loading.traits) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Analyzing traits...</p>
      </div>
    );
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Trait Analysis"
        description={`${filtered.length} traits analyzed`}
      />

      <div className="flex gap-1 mb-6 flex-wrap">
        <button
          onClick={() => setSelectedCategory('All')}
          className={`px-3 py-1.5 text-xs rounded-sm transition-colors duration-100 ${
            selectedCategory === 'All'
              ? 'bg-accent text-white'
              : 'text-text-muted hover:text-text border border-border'
          }`}
        >
          All
        </button>
        {TRAIT_CATEGORIES.map((cat) => (
          <button
            key={cat}
            onClick={() => setSelectedCategory(cat)}
            className={`px-3 py-1.5 text-xs rounded-sm transition-colors duration-100 ${
              selectedCategory === cat
                ? 'bg-accent text-white'
                : 'text-text-muted hover:text-text border border-border'
            }`}
          >
            {cat}
          </button>
        ))}
      </div>

      <div className="grid grid-cols-3 gap-4">
        {filtered.map((trait) => (
          <Card key={trait.name}>
            <div className="mb-2">
              <div className="flex items-center gap-2">
                <p className="text-sm font-medium text-text">{trait.name}</p>
                {trait.source && trait.source !== 'curated' && (
                  <span className="text-[9px] text-text-muted font-mono uppercase tracking-wider">
                    {trait.source}
                  </span>
                )}
              </div>
              <p className="text-[10px] uppercase tracking-wider text-text-muted">
                {trait.category}
              </p>
            </div>

            <p className="text-sm font-mono text-accent mb-2">{trait.prediction}</p>

            <div className="mb-2">
              <div className="flex items-center justify-between mb-1">
                <span className="text-[10px] text-text-muted">Confidence</span>
                <span className="text-[10px] font-mono text-text-muted">
                  {(trait.confidence * 100).toFixed(0)}%
                </span>
              </div>
              <div className="w-full h-1 bg-border rounded-full overflow-hidden">
                <div
                  className="h-full bg-accent rounded-full"
                  style={{ width: `${trait.confidence * 100}%` }}
                />
              </div>
            </div>

            <p className="text-xs text-text-muted leading-relaxed">{trait.description}</p>

            {trait.contributingSnps.length > 0 && (
              <div className="mt-2 pt-2 border-t border-border flex items-center gap-2 text-[10px]">
                <span className="font-mono text-accent">{trait.contributingSnps[0].rsid}</span>
                <span className="font-mono text-text-muted">{trait.contributingSnps[0].genotype}</span>
              </div>
            )}
          </Card>
        ))}
      </div>

      {filtered.length === 0 && (
        <p className="text-sm text-text-muted text-center py-8">
          No traits found for this category.
        </p>
      )}
    </motion.div>
  );
}
