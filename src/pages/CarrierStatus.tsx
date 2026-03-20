import { useEffect } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useAnalysisStore } from '../stores/analysisStore';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

const STATUS_CONFIG: Record<string, { label: string; color: string; bg: string }> = {
  not_carrier: {
    label: 'Clear',
    color: '#4A7C59',
    bg: 'rgba(74, 124, 89, 0.08)',
  },
  carrier: {
    label: 'Carrier',
    color: '#C4A35A',
    bg: 'rgba(196, 163, 90, 0.08)',
  },
  affected: {
    label: 'Affected',
    color: '#A94442',
    bg: 'rgba(169, 68, 66, 0.08)',
  },
};

export default function CarrierStatus() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const { carrierStatus, loading, loadCarrierStatus } = useAnalysisStore();

  useEffect(() => {
    if (activeGenomeId) {
      loadCarrierStatus(activeGenomeId);
    }
  }, [activeGenomeId, loadCarrierStatus]);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to view carrier status.</p>
      </div>
    );
  }

  if (loading.carrierStatus) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Analyzing carrier status...</p>
      </div>
    );
  }

  const clearCount = carrierStatus?.filter((c) => c.status === 'not_carrier').length ?? 0;
  const carrierCount = carrierStatus?.filter((c) => c.status === 'carrier').length ?? 0;
  const affectedCount = carrierStatus?.filter((c) => c.status === 'affected').length ?? 0;

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Carrier Status"
        description="Recessive condition carrier screening results"
      />

      <div className="flex gap-6 mb-8">
        <div className="flex items-center gap-2">
          <span className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: STATUS_CONFIG.not_carrier.color }} />
          <span className="text-xs text-text-muted">{clearCount} Clear</span>
        </div>
        <div className="flex items-center gap-2">
          <span className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: STATUS_CONFIG.carrier.color }} />
          <span className="text-xs text-text-muted">{carrierCount} Carrier</span>
        </div>
        <div className="flex items-center gap-2">
          <span className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: STATUS_CONFIG.affected.color }} />
          <span className="text-xs text-text-muted">{affectedCount} Affected</span>
        </div>
      </div>

      <div className="space-y-3">
        {carrierStatus?.map((result) => {
          const config = STATUS_CONFIG[result.status] ?? STATUS_CONFIG.not_carrier;
          return (
            <Card key={result.condition}>
              <div className="flex items-start gap-4">
                <div
                  className="w-2 h-2 rounded-full mt-1.5 shrink-0"
                  style={{ backgroundColor: config.color }}
                />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center justify-between mb-1">
                    <div className="flex items-center gap-2">
                      <p className="text-sm font-medium text-text">{result.condition}</p>
                      {result.source && result.source !== 'curated' && (
                        <span className="text-[9px] text-text-muted font-mono uppercase tracking-wider">
                          {result.source}
                        </span>
                      )}
                    </div>
                    <span
                      className="text-[10px] font-semibold px-2 py-0.5 rounded-sm"
                      style={{ color: config.color, backgroundColor: config.bg }}
                    >
                      {config.label}
                    </span>
                  </div>
                  <p className="text-xs text-text-muted mb-2">{result.description}</p>
                  <div className="flex items-center gap-4 text-[10px] text-text-muted">
                    <span>
                      Gene: <span className="font-mono">{result.gene}</span>
                    </span>
                    <span>
                      Inheritance: {result.inheritancePattern}
                    </span>
                    {result.variantsChecked.length > 0 && (
                      <span>
                        {result.variantsChecked.map((v) => (
                          <span key={v.rsid}>
                            <span className="font-mono text-accent">{v.rsid}</span>
                            {' '}&middot;{' '}
                            <span className="font-mono">{v.genotype}</span>
                            {' '}
                          </span>
                        ))}
                      </span>
                    )}
                  </div>
                </div>
              </div>
            </Card>
          );
        })}
      </div>

      {carrierStatus?.length === 0 && (
        <p className="text-sm text-text-muted text-center py-8">
          No carrier screening data available.
        </p>
      )}
    </motion.div>
  );
}
