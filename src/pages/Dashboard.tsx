import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useTauriCommand } from '../hooks/useTauriCommand';
import type { GenomeSummary, ReferenceDbStatus } from '../lib/tauri-bridge';
import { getReferenceStatus } from '../lib/tauri-bridge';
import { formatNumber, formatPercentage } from '../lib/formatters';
import MetricCard from '../design-system/components/MetricCard';
import ChromosomeIdeogram from '../design-system/components/ChromosomeIdeogram';
import SmallMultiple from '../design-system/components/SmallMultiple';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

export default function Dashboard() {
  const { activeGenomeId, loadGenomes, genomes } = useGenomeStore();
  const [refStatus, setRefStatus] = useState<ReferenceDbStatus[] | null>(null);

  useEffect(() => {
    loadGenomes();
    getReferenceStatus()
      .then(setRefStatus)
      .catch(() => {});
  }, [loadGenomes]);

  const { data: summary, loading } = useTauriCommand<GenomeSummary>(
    'get_genome_summary',
    activeGenomeId ? { genomeId: activeGenomeId } : undefined,
  );

  if (!activeGenomeId || genomes.length === 0) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <div className="text-center">
          <p className="text-sm text-text-muted mb-2">No genome data loaded</p>
          <a
            href="/import"
            className="text-sm text-accent hover:underline"
          >
            Import your genome data
          </a>
        </div>
      </div>
    );
  }

  if (loading || !summary) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Loading genome summary...</p>
      </div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.3 }}
    >
      <SectionHeader
        title="Genome Overview"
        description={`Build ${summary.genome.build ?? 'Unknown'} \u00b7 Imported ${summary.genome.importedAt}`}
      />

      {refStatus && refStatus.every((s) => s.status !== 'ready') && (
        <div className="mb-6 px-4 py-3 border border-border rounded-sm bg-surface">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm text-text">Reference databases not downloaded</p>
              <p className="text-xs text-text-muted">
                Analysis results are limited to a small set of curated variants.
                Download reference databases to unlock thousands more findings.
              </p>
            </div>
            <a href="/settings" className="text-sm text-accent hover:underline shrink-0 ml-4">
              Download Now
            </a>
          </div>
        </div>
      )}

      <div className="grid grid-cols-3 gap-4 mb-8">
        <MetricCard
          label="Total SNPs"
          value={formatNumber(summary.totalSnps)}
          description="Single nucleotide polymorphisms analyzed"
        />
        <MetricCard
          label="Heterozygosity Rate"
          value={formatPercentage(summary.heterozygosityRate)}
          description="Proportion of heterozygous loci"
        />
        <MetricCard
          label="Missing Data"
          value={`${summary.missingDataPercent}%`}
          description="Percentage of no-call genotypes"
        />
      </div>

      <SectionHeader title="Chromosome Distribution" />
      <Card className="mb-8 p-6">
        <ChromosomeIdeogram chromosomeData={summary.chromosomeCounts} />
      </Card>

      <SmallMultiple columns={4} title="SNPs per Chromosome">
        {summary.chromosomeCounts.map((chr) => (
          <Card key={chr.chromosome}>
            <div className="text-center py-1">
              <p className="text-xs text-text-muted mb-0.5 font-mono">
                chr{chr.chromosome}
              </p>
              <p className="text-lg font-mono text-text">
                {formatNumber(chr.count)}
              </p>
            </div>
          </Card>
        ))}
      </SmallMultiple>
    </motion.div>
  );
}
