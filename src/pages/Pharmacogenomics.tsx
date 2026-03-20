import { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useAnalysisStore } from '../stores/analysisStore';
import type { PharmaResult } from '../lib/tauri-bridge';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

const PHENOTYPE_COLORS: Record<string, string> = {
  'ultra-rapid': '#2D5F8A',
  normal: '#4A7C59',
  intermediate: '#C4A35A',
  poor: '#A94442',
};

const PHENOTYPE_LABELS: Record<string, string> = {
  'ultra-rapid': 'Ultra-rapid',
  normal: 'Normal',
  intermediate: 'Intermediate',
  poor: 'Poor',
};

const ACTIONABILITY_STYLES: Record<string, string> = {
  high: 'bg-risk-high/10 text-risk-high border-risk-high/30',
  moderate: 'bg-risk-moderate/10 text-risk-moderate border-risk-moderate/30',
  low: 'bg-border text-text-muted border-border',
};

export default function Pharmacogenomics() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const { pharmacogenomics, loading, loadPharmacogenomics } = useAnalysisStore();
  const [expandedEnzyme, setExpandedEnzyme] = useState<string | null>(null);

  useEffect(() => {
    if (activeGenomeId) {
      loadPharmacogenomics(activeGenomeId);
    }
  }, [activeGenomeId, loadPharmacogenomics]);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to view pharmacogenomics.</p>
      </div>
    );
  }

  if (loading.pharmacogenomics) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Analyzing pharmacogenomics...</p>
      </div>
    );
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Pharmacogenomics"
        description="Drug metabolism enzyme analysis and clinical recommendations"
      />

      <div className="grid grid-cols-2 gap-4">
        {pharmacogenomics?.map((result) => (
          <EnzymeCard
            key={result.gene}
            result={result}
            isExpanded={expandedEnzyme === result.gene}
            onToggle={() =>
              setExpandedEnzyme(expandedEnzyme === result.gene ? null : result.gene)
            }
          />
        ))}
      </div>

      {pharmacogenomics?.length === 0 && (
        <p className="text-sm text-text-muted text-center py-8">
          No pharmacogenomic data available.
        </p>
      )}
    </motion.div>
  );
}

function EnzymeCard({
  result,
  isExpanded,
  onToggle,
}: {
  result: PharmaResult;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const color = PHENOTYPE_COLORS[result.phenotype] ?? '#6B6965';

  return (
    <Card onClick={onToggle}>
      <div className="flex items-start justify-between mb-2">
        <div className="flex items-center gap-2">
          <p className="text-sm font-semibold text-text">{result.gene}</p>
          {result.source && result.source !== 'curated' && (
            <span className="text-[9px] text-text-muted font-mono uppercase tracking-wider">
              {result.source}
            </span>
          )}
        </div>
        <span
          className={`text-[10px] px-2 py-0.5 rounded-sm border ${
            ACTIONABILITY_STYLES[result.clinicalActionability]
          }`}
        >
          {result.clinicalActionability} actionability
        </span>
      </div>

      <div className="flex items-center gap-3 mb-2">
        <span className="text-xs font-mono text-text-muted">
          {result.starAllele}
        </span>
        <span
          className="text-xs font-semibold px-2 py-0.5 rounded-sm"
          style={{ color, backgroundColor: `${color}15` }}
        >
          {PHENOTYPE_LABELS[result.phenotype] ?? result.phenotype}
        </span>
      </div>

      <p className="text-xs text-text-muted">{result.phenotype} metabolizer</p>

      <AnimatePresence>
        {isExpanded && result.affectedDrugs.length > 0 && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="mt-4 pt-3 border-t border-border">
              <p className="text-[10px] uppercase tracking-wider text-text-muted font-semibold mb-2">
                Affected Drugs
              </p>
              <div className="space-y-2">
                {result.affectedDrugs.map((drug) => (
                  <div key={drug.name} className="text-xs">
                    <div className="flex items-center justify-between">
                      <span className="font-medium text-text">{drug.name}</span>
                      <span className="text-[10px] text-text-muted font-mono">
                        {drug.evidenceLevel}
                      </span>
                    </div>
                    <p className="text-text-muted mt-0.5">{drug.recommendation}</p>
                  </div>
                ))}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </Card>
  );
}
