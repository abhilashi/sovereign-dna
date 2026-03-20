import { useState, useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { useSnpData } from '../hooks/useSnpData';
import { exportSnps, getSnpDetail } from '../lib/tauri-bridge';
import type { SnpDetail } from '../lib/tauri-bridge';
import { CHROMOSOMES } from '../lib/constants';
import { formatNumber, formatChromosome, formatGenotype } from '../lib/formatters';
import SectionHeader from '../design-system/components/SectionHeader';
import DataTable from '../design-system/components/DataTable';
import Card from '../design-system/components/Card';

export default function Explorer() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const {
    snps,
    total,
    loading,
    page,
    pageSize,
    setPage,
    setSearch,
    setChromosomeFilter,
    search,
    chromosomeFilter,
  } = useSnpData(activeGenomeId);

  const [selectedSnp, setSelectedSnp] = useState<SnpDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [exporting, setExporting] = useState(false);

  const handleRowClick = useCallback(
    async (row: Record<string, unknown>) => {
      if (!activeGenomeId) return;
      setDetailLoading(true);
      try {
        const detail = await getSnpDetail(activeGenomeId, row.rsid as string);
        setSelectedSnp(detail);
      } catch {
        setSelectedSnp(null);
      } finally {
        setDetailLoading(false);
      }
    },
    [activeGenomeId],
  );

  const handleExport = useCallback(
    async (format: 'csv' | 'json') => {
      if (!activeGenomeId) return;
      setExporting(true);
      try {
        await exportSnps(activeGenomeId, format);
      } finally {
        setExporting(false);
      }
    },
    [activeGenomeId],
  );

  const totalPages = Math.ceil(total / pageSize);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to explore SNPs.</p>
      </div>
    );
  }

  const columns = [
    { key: 'rsid', label: 'rsID', width: 120 },
    {
      key: 'chromosome',
      label: 'Chr',
      width: 60,
      render: (v: unknown) => formatChromosome(String(v)),
    },
    { key: 'position', label: 'Position', width: 110, render: (v: unknown) => formatNumber(Number(v)) },
    { key: 'genotype', label: 'Genotype', width: 80, render: (v: unknown) => formatGenotype(String(v)) },
  ];

  const tableData = snps.map((s) => ({
    rsid: s.rsid,
    chromosome: s.chromosome,
    position: s.position,
    genotype: s.genotype,
  }));

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.3 }}
      className="flex flex-col h-[calc(100vh-4rem)]"
    >
      <SectionHeader
        title="SNP Explorer"
        description={`${formatNumber(total)} variants`}
        action={
          <div className="flex gap-2">
            <button
              onClick={() => handleExport('csv')}
              disabled={exporting}
              className="px-3 py-1.5 text-xs border border-border rounded-sm text-text-muted hover:text-text hover:border-accent transition-colors duration-100 disabled:opacity-50"
            >
              Export CSV
            </button>
            <button
              onClick={() => handleExport('json')}
              disabled={exporting}
              className="px-3 py-1.5 text-xs border border-border rounded-sm text-text-muted hover:text-text hover:border-accent transition-colors duration-100 disabled:opacity-50"
            >
              Export JSON
            </button>
          </div>
        }
      />

      <div className="flex gap-3 mb-4">
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search rsID, gene, or chromosome..."
          className="flex-1 px-3 py-2 text-sm border border-border rounded-sm bg-surface text-text placeholder:text-text-muted focus:outline-none focus:border-accent font-mono"
        />
        <select
          value={chromosomeFilter ?? ''}
          onChange={(e) => setChromosomeFilter(e.target.value || null)}
          className="px-3 py-2 text-sm border border-border rounded-sm bg-surface text-text focus:outline-none focus:border-accent"
        >
          <option value="">All chromosomes</option>
          {CHROMOSOMES.map((chr) => (
            <option key={chr} value={chr}>
              Chr {chr}
            </option>
          ))}
        </select>
      </div>

      <div className="flex gap-4 flex-1 min-h-0">
        <div className="flex-1 min-w-0 flex flex-col">
          {loading ? (
            <div className="flex items-center justify-center flex-1">
              <p className="text-sm text-text-muted">Loading...</p>
            </div>
          ) : (
            <div className="flex-1 min-h-0">
              <DataTable
                columns={columns}
                data={tableData}
                onRowClick={handleRowClick}
                rowHeight={36}
              />
            </div>
          )}

          <div className="flex items-center justify-between pt-3 mt-auto">
            <p className="text-xs text-text-muted">
              Page {page + 1} of {totalPages}
            </p>
            <div className="flex gap-2">
              <button
                onClick={() => setPage(Math.max(0, page - 1))}
                disabled={page === 0}
                className="px-3 py-1 text-xs border border-border rounded-sm text-text-muted hover:text-text disabled:opacity-30 transition-colors"
              >
                Previous
              </button>
              <button
                onClick={() => setPage(Math.min(totalPages - 1, page + 1))}
                disabled={page >= totalPages - 1}
                className="px-3 py-1 text-xs border border-border rounded-sm text-text-muted hover:text-text disabled:opacity-30 transition-colors"
              >
                Next
              </button>
            </div>
          </div>
        </div>

        <AnimatePresence>
          {(selectedSnp || detailLoading) && (
            <motion.div
              initial={{ opacity: 0, width: 0 }}
              animate={{ opacity: 1, width: 280 }}
              exit={{ opacity: 0, width: 0 }}
              transition={{ duration: 0.2 }}
              className="shrink-0 overflow-hidden"
            >
              <Card className="h-full">
                {detailLoading ? (
                  <p className="text-xs text-text-muted">Loading...</p>
                ) : selectedSnp ? (
                  <div>
                    <div className="flex items-center justify-between mb-4">
                      <p className="text-sm font-mono text-accent font-semibold">
                        {selectedSnp.snp.rsid}
                      </p>
                      <button
                        onClick={() => setSelectedSnp(null)}
                        className="text-xs text-text-muted hover:text-text"
                      >
                        Close
                      </button>
                    </div>
                    <div className="space-y-3 text-xs">
                      <div>
                        <p className="text-text-muted uppercase tracking-wider text-[10px]">
                          Chromosome
                        </p>
                        <p className="font-mono text-text">
                          {formatChromosome(selectedSnp.snp.chromosome)}
                        </p>
                      </div>
                      <div>
                        <p className="text-text-muted uppercase tracking-wider text-[10px]">
                          Position
                        </p>
                        <p className="font-mono text-text">
                          {formatNumber(selectedSnp.snp.position)}
                        </p>
                      </div>
                      <div>
                        <p className="text-text-muted uppercase tracking-wider text-[10px]">
                          Genotype
                        </p>
                        <p className="font-mono text-text">
                          {formatGenotype(selectedSnp.snp.genotype)}
                        </p>
                      </div>
                      {selectedSnp.annotations.length > 0 && (
                        <div>
                          <p className="text-text-muted uppercase tracking-wider text-[10px] mb-1">
                            Annotations
                          </p>
                          <div className="space-y-2">
                            {selectedSnp.annotations.map((ann, i) => (
                              <div key={i} className="text-text-muted leading-relaxed">
                                {ann.gene && (
                                  <p className="font-mono text-text">{ann.gene}</p>
                                )}
                                {ann.clinicalSignificance && (
                                  <p>{ann.clinicalSignificance}</p>
                                )}
                                {ann.condition && (
                                  <p>{ann.condition}</p>
                                )}
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                ) : null}
              </Card>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
}
