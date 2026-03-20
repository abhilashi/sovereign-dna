import { useEffect, useState, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useAnalysisStore } from '../stores/analysisStore';
import type { HealthRiskResult } from '../lib/tauri-bridge';
import { HEALTH_CATEGORIES } from '../lib/constants';
import SectionHeader from '../design-system/components/SectionHeader';
import SmallMultiple from '../design-system/components/SmallMultiple';
import RiskBar from '../design-system/components/RiskBar';
import Card from '../design-system/components/Card';

const RISK_ORDER: Record<string, number> = { high: 0, elevated: 1, moderate: 2, low: 3 };

export default function HealthRisks() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const { healthRisks, loading, loadHealthRisks } = useAnalysisStore();
  const [selectedCategory, setSelectedCategory] = useState<string>('All');
  const [expandedCondition, setExpandedCondition] = useState<string | null>(null);

  useEffect(() => {
    if (activeGenomeId) {
      loadHealthRisks(activeGenomeId);
    }
  }, [activeGenomeId, loadHealthRisks]);

  const filtered = useMemo(() => {
    if (!healthRisks) return [];
    let list = [...healthRisks];
    if (selectedCategory !== 'All') {
      list = list.filter((r) => r.category === selectedCategory);
    }
    list.sort((a, b) => (RISK_ORDER[a.riskLevel] ?? 4) - (RISK_ORDER[b.riskLevel] ?? 4));
    return list;
  }, [healthRisks, selectedCategory]);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to view health risks.</p>
      </div>
    );
  }

  if (loading.healthRisks) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Analyzing health risks...</p>
      </div>
    );
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Health Risk Analysis"
        description={`${filtered.length} conditions analyzed`}
      />

      <div className="flex gap-1 mb-6 flex-wrap">
        {HEALTH_CATEGORIES.map((cat) => (
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

      <SmallMultiple columns={1} title="Risk Assessment">
        {filtered.map((risk) => (
          <div key={risk.condition}>
            <Card
              onClick={() =>
                setExpandedCondition(
                  expandedCondition === risk.condition ? null : risk.condition,
                )
              }
            >
              <RiskBar
                level={risk.riskLevel as import('../design-system/tokens').RiskLevel}
                score={risk.score}
                label={risk.condition}
              />
              <div className="flex items-center gap-2 mt-1 pl-[172px]">
                <p className="text-xs text-text-muted">
                  {risk.category} &middot; Confidence: {risk.confidence} &middot; Studies: {risk.studyCount}
                </p>
                {risk.source && risk.source !== 'curated' && (
                  <span className="text-[9px] text-text-muted font-mono uppercase tracking-wider">
                    {risk.source}
                  </span>
                )}
              </div>
            </Card>

            <AnimatePresence>
              {expandedCondition === risk.condition && (
                <ConditionDetail risk={risk} />
              )}
            </AnimatePresence>
          </div>
        ))}
      </SmallMultiple>

      {filtered.length === 0 && (
        <p className="text-sm text-text-muted text-center py-8">
          No results found for this category.
        </p>
      )}
    </motion.div>
  );
}

function ConditionDetail({ risk }: { risk: HealthRiskResult }) {
  return (
    <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: 'auto' }}
      exit={{ opacity: 0, height: 0 }}
      transition={{ duration: 0.2 }}
      className="overflow-hidden"
    >
      <div className="mx-5 mb-4 p-4 border-l-2 border-border">
        <p className="text-xs font-semibold uppercase tracking-wider text-text-muted mb-3">
          Contributing SNPs
        </p>
        <div className="space-y-2">
          {risk.contributingSnps.map((snp) => (
            <div
              key={snp.rsid}
              className="flex items-center gap-4 text-xs"
            >
              <span className="font-mono text-accent w-24">{snp.rsid}</span>
              <span className="font-mono text-text w-16">{snp.genotype}</span>
              <span className="text-text-muted w-20">{snp.gene}</span>
              <span className="text-text-muted">
                Risk allele: <span className="font-mono">{snp.riskAllele}</span>
              </span>
              <span className="text-text-muted">
                {snp.effect}
              </span>
            </div>
          ))}
        </div>
      </div>
    </motion.div>
  );
}
