import { useEffect, useState, useCallback } from 'react';
import { motion } from 'framer-motion';
import { useSettingsStore } from '../stores/settingsStore';
import { useGenomeStore } from '../stores/genomeStore';
import { formatDate, formatNumber } from '../lib/formatters';
import {
  getReferenceStatus,
  downloadReferenceDatabase,
  deleteReferenceDatabase as deleteRefDb,
} from '../lib/tauri-bridge';
import type { ReferenceDbStatus, ReferenceProgress } from '../lib/tauri-bridge';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

const REFERENCE_DATABASES = [
  {
    source: 'clinvar',
    name: 'ClinVar',
    description:
      'Clinical variant classifications from NCBI. Links SNPs to disease risk, pathogenicity, and clinical significance.',
    size: '~80 MB download',
  },
  {
    source: 'gwas_catalog',
    name: 'GWAS Catalog',
    description:
      'Genome-wide association study results from EBI. Connects your variants to traits, conditions, and research findings.',
    size: '~50 MB download',
  },
  {
    source: 'snpedia',
    name: 'SNPedia',
    description:
      'Community-curated SNP annotations. Plain-language summaries of what your variants mean.',
    size: 'Fetched via API',
  },
];

interface DownloadState {
  source: string;
  progress: number;
  message: string;
  phase: string;
}

export default function Settings() {
  const { researchUpdateFrequency, setResearchUpdateFrequency } = useSettingsStore();
  const { genomes, loadGenomes, deleteGenome, activeGenomeId } = useGenomeStore();
  const [refStatuses, setRefStatuses] = useState<ReferenceDbStatus[]>([]);
  const [downloading, setDownloading] = useState<Record<string, DownloadState>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    loadGenomes();
    loadRefStatus();
  }, [loadGenomes]);

  const loadRefStatus = useCallback(async () => {
    try {
      const statuses = await getReferenceStatus();
      setRefStatuses(statuses);
    } catch {
      // Status not yet available; leave empty
    }
  }, []);

  function getStatusForSource(source: string): ReferenceDbStatus | undefined {
    return refStatuses.find((s) => s.source === source);
  }

  async function handleDownload(source: string) {
    setDownloading((prev) => ({
      ...prev,
      [source]: { source, progress: 0, message: 'Starting download...', phase: 'downloading' },
    }));
    setErrors((prev) => {
      const next = { ...prev };
      delete next[source];
      return next;
    });

    try {
      await downloadReferenceDatabase(source, activeGenomeId ?? null, (p: ReferenceProgress) => {
        setDownloading((prev) => ({
          ...prev,
          [source]: {
            source: p.source,
            progress: p.progress,
            message: p.message,
            phase: p.phase,
          },
        }));
      });
      setDownloading((prev) => {
        const next = { ...prev };
        delete next[source];
        return next;
      });
      await loadRefStatus();
    } catch (err) {
      setDownloading((prev) => {
        const next = { ...prev };
        delete next[source];
        return next;
      });
      setErrors((prev) => ({ ...prev, [source]: String(err) }));
    }
  }

  async function handleDelete(source: string) {
    if (!confirm(`Delete ${REFERENCE_DATABASES.find((d) => d.source === source)?.name} database?`)) {
      return;
    }
    try {
      await deleteRefDb(source);
      await loadRefStatus();
    } catch (err) {
      setErrors((prev) => ({ ...prev, [source]: String(err) }));
    }
  }

  async function handleDownloadAll() {
    for (const db of REFERENCE_DATABASES) {
      const status = getStatusForSource(db.source);
      if (!status || status.status !== 'ready') {
        await handleDownload(db.source);
      }
    }
  }

  const isAnyDownloading = Object.keys(downloading).length > 0;

  function renderStatusBadge(source: string) {
    const dl = downloading[source];
    if (dl) {
      return (
        <span className="flex items-center gap-1.5 text-xs text-accent">
          <span className="inline-block w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
          {dl.phase === 'downloading' ? 'Downloading...' : 'Parsing...'}
        </span>
      );
    }

    const err = errors[source];
    if (err) {
      return (
        <span className="flex items-center gap-1.5 text-xs text-risk-high">
          <span className="inline-block w-1.5 h-1.5 rounded-full bg-risk-high" />
          Error
        </span>
      );
    }

    const status = getStatusForSource(source);
    if (status?.status === 'ready') {
      return (
        <span className="flex items-center gap-1.5 text-xs text-green-600 dark:text-green-400">
          <span className="inline-block w-1.5 h-1.5 rounded-full bg-green-600 dark:bg-green-400" />
          Ready
        </span>
      );
    }

    return (
      <span className="flex items-center gap-1.5 text-xs text-text-muted">
        <span className="inline-block w-1.5 h-1.5 rounded-full bg-text-muted opacity-50" />
        Not Downloaded
      </span>
    );
  }

  function renderActions(source: string) {
    const dl = downloading[source];
    if (dl) return null;

    const err = errors[source];
    const status = getStatusForSource(source);

    if (err) {
      return (
        <button
          onClick={() => handleDownload(source)}
          className="text-xs text-accent hover:underline"
        >
          Retry
        </button>
      );
    }

    if (status?.status === 'ready') {
      return (
        <div className="flex items-center gap-3">
          <button
            onClick={() => handleDownload(source)}
            disabled={isAnyDownloading}
            className="text-xs text-accent hover:underline disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Update
          </button>
          <button
            onClick={() => handleDelete(source)}
            disabled={isAnyDownloading}
            className="text-xs text-text-muted hover:text-risk-high transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Delete
          </button>
        </div>
      );
    }

    return (
      <button
        onClick={() => handleDownload(source)}
        disabled={isAnyDownloading}
        className="text-xs text-accent hover:underline disabled:opacity-40 disabled:cursor-not-allowed"
      >
        Download
      </button>
    );
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader title="Settings" />

      <div className="max-w-lg space-y-8">
        {/* Reference Databases */}
        <div>
          <SectionHeader
            title="Reference Databases"
            description="Download open genomic databases to enrich your analysis. Your genome data stays local — only public database files are downloaded."
          />
          <Card>
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-border text-text-muted text-left">
                  <th className="pb-2 font-medium">Database</th>
                  <th className="pb-2 font-medium">Status</th>
                  <th className="pb-2 font-medium text-right">Records</th>
                  <th className="pb-2 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {REFERENCE_DATABASES.map((db) => {
                  const status = getStatusForSource(db.source);
                  const dl = downloading[db.source];
                  const err = errors[db.source];

                  return (
                    <tr key={db.source} className="border-b border-border last:border-b-0">
                      <td className="py-2.5">
                        <span className="text-sm text-text font-medium">{db.name}</span>
                        {dl && (
                          <div className="mt-1">
                            <div className="w-32 h-1 bg-border rounded-full overflow-hidden">
                              <motion.div className="h-full bg-accent rounded-full" animate={{ width: `${Math.round(dl.progress * 100)}%` }} transition={{ duration: 0.3 }} />
                            </div>
                            <p className="text-[10px] text-text-muted font-mono mt-0.5">{dl.message}</p>
                          </div>
                        )}
                        {err && !dl && <p className="text-[10px] text-risk-high mt-0.5">{err}</p>}
                      </td>
                      <td className="py-2.5">{renderStatusBadge(db.source)}</td>
                      <td className="py-2.5 text-right font-mono text-text-muted">
                        {status?.status === 'ready' ? formatNumber(status.recordCount) : '\u2014'}
                      </td>
                      <td className="py-2.5 text-right">{renderActions(db.source)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            <div className="flex justify-end mt-3 pt-2 border-t border-border">
              <button
                onClick={handleDownloadAll}
                disabled={isAnyDownloading}
                className="text-xs text-accent hover:underline disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Download All
              </button>
            </div>
          </Card>
        </div>

        {/* Research Updates */}
        <div>
          <Card>
            <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-4">
              Research Updates
            </p>
            <div className="space-y-2">
              {(['daily', 'weekly', 'monthly', 'manual'] as const).map((freq) => (
                <label key={freq} className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="radio"
                    name="frequency"
                    checked={researchUpdateFrequency === freq}
                    onChange={() => setResearchUpdateFrequency(freq)}
                    className="accent-accent"
                  />
                  <span className="text-sm text-text capitalize">{freq}</span>
                </label>
              ))}
            </div>
          </Card>
        </div>

        {/* Data Management */}
        <div>
          <SectionHeader
            title="Data Management"
            description="Imported genome files stored locally"
          />
          {genomes.length === 0 ? (
            <p className="text-sm text-text-muted">No genomes imported.</p>
          ) : (
            <div className="space-y-2">
              {genomes.map((genome) => (
                <Card key={genome.id}>
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="text-sm text-text">{genome.filename}</p>
                      <div className="flex items-center gap-3 text-xs text-text-muted mt-0.5">
                        <span className="font-mono">{formatNumber(genome.snpCount)} SNPs</span>
                        <span>{genome.format}</span>
                        <span>{genome.build ?? 'Unknown'}</span>
                        <span>{formatDate(genome.importedAt)}</span>
                      </div>
                    </div>
                    <button
                      onClick={() => {
                        if (confirm(`Delete ${genome.filename}?`)) {
                          deleteGenome(genome.id);
                        }
                      }}
                      className="text-xs text-text-muted hover:text-risk-high transition-colors"
                    >
                      Delete
                    </button>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </div>

        {/* About */}
        <div>
          <SectionHeader title="About" />
          <Card>
            <div className="space-y-3 text-xs text-text-muted leading-relaxed">
              <p>
                <span className="font-semibold text-text">Genome Studio</span> is a
                fully local DNA analysis application. All data processing happens
                entirely on your device.
              </p>
              <p>
                No genetic data is transmitted over the network. Research article
                matching uses only public rsID identifiers, never your personal
                genotype data.
              </p>
              <p>
                This tool is for educational and informational purposes only. It is
                not a medical device and should not be used for clinical
                decision-making.
              </p>
            </div>
          </Card>
        </div>
      </div>
    </motion.div>
  );
}
