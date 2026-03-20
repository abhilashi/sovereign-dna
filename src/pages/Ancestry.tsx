import { useEffect } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useAnalysisStore } from '../stores/analysisStore';
import { colors } from '../design-system/tokens';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';
import WaffleChart from '../design-system/components/WaffleChart';

const ANCESTRY_COLORS = [
  colors.ancestry.ochre,
  colors.ancestry.terracotta,
  colors.ancestry.forest,
  colors.ancestry.slate,
  colors.ancestry.rust,
  '#8B7355',
  '#6B8E9B',
  '#9B7B6B',
];

export default function Ancestry() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const { ancestry, loading, loadAncestry } = useAnalysisStore();

  useEffect(() => {
    if (activeGenomeId) {
      loadAncestry(activeGenomeId);
    }
  }, [activeGenomeId, loadAncestry]);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to view ancestry.</p>
      </div>
    );
  }

  if (loading.ancestry) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Analyzing ancestry composition...</p>
      </div>
    );
  }

  if (!ancestry) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">No ancestry data available.</p>
      </div>
    );
  }

  const waffleData = ancestry.populations.map((comp, i) => ({
    name: comp.name,
    percentage: comp.percentage,
    color: comp.color || ANCESTRY_COLORS[i % ANCESTRY_COLORS.length],
  }));

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Ancestry Composition"
        description="Estimated genetic ancestry based on reference populations"
      />

      <div className="grid grid-cols-2 gap-8 mb-8">
        <Card className="p-6">
          <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-4">
            Composition
          </p>
          <WaffleChart data={waffleData} />
        </Card>

        <div className="space-y-4">
          <Card className="p-6">
            <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-3">
              Haplogroups
            </p>
            <div className="space-y-3">
              <div>
                <p className="text-[10px] text-text-muted uppercase tracking-wider">
                  Maternal (mtDNA)
                </p>
                <p className="text-lg font-mono text-text">
                  {ancestry.maternalHaplogroup}
                </p>
              </div>
              {ancestry.paternalHaplogroup && (
                <div>
                  <p className="text-[10px] text-text-muted uppercase tracking-wider">
                    Paternal (Y-DNA)
                  </p>
                  <p className="text-lg font-mono text-text">
                    {ancestry.paternalHaplogroup}
                  </p>
                </div>
              )}
            </div>
          </Card>

          <Card className="p-6">
            <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-3">
              Breakdown
            </p>
            <div className="space-y-2">
              {[...ancestry.populations]
                .sort((a, b) => b.percentage - a.percentage)
                .map((comp, i) => (
                  <div key={comp.name} className="flex items-center gap-3">
                    <span
                      className="w-2 h-2 rounded-sm shrink-0"
                      style={{
                        backgroundColor: comp.color || ANCESTRY_COLORS[i % ANCESTRY_COLORS.length],
                      }}
                    />
                    <span className="text-xs text-text flex-1">{comp.name}</span>
                    <span className="text-xs font-mono text-text w-14 text-right">
                      {comp.percentage.toFixed(1)}%
                    </span>
                  </div>
                ))}
            </div>
          </Card>
        </div>
      </div>

    </motion.div>
  );
}
