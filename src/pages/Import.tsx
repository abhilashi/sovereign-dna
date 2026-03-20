import { useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion, AnimatePresence } from 'framer-motion';
import { open } from '@tauri-apps/plugin-dialog';
import { useFileDrop } from '../hooks/useFileDrop';
import { useGenomeStore } from '../stores/genomeStore';
import { importGenome, downloadReferenceDatabase, getReferenceStatus } from '../lib/tauri-bridge';
import type { ImportResult, ImportProgress, ReferenceProgress } from '../lib/tauri-bridge';
import { formatNumber, formatPercentage } from '../lib/formatters';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

type ImportStage = 'idle' | 'importing' | 'complete' | 'downloading_refs' | 'all_done' | 'error';

const REF_DATABASES = [
  { source: 'clinvar', label: 'ClinVar' },
  { source: 'gwas_catalog', label: 'GWAS Catalog' },
  { source: 'snpedia', label: 'SNPedia' },
];

export default function Import() {
  const navigate = useNavigate();
  const dropRef = useRef<HTMLDivElement>(null);
  const { isDragging, error: dropError } = useFileDrop(dropRef);
  const loadGenomes = useGenomeStore((s) => s.loadGenomes);

  const [stage, setStage] = useState<ImportStage>('idle');
  const [progress, setProgress] = useState(0);
  const [progressMessage, setProgressMessage] = useState('');
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Reference download state
  const [refCurrent, setRefCurrent] = useState('');
  const [refProgress, setRefProgress] = useState(0);
  const [refMessage, setRefMessage] = useState('');
  const [refCompleted, setRefCompleted] = useState<string[]>([]);
  const [refErrors, setRefErrors] = useState<string[]>([]);

  async function handleFileSelect() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'Genome Data', extensions: ['txt', 'csv', 'tsv', 'zip'] }],
      });
      if (selected) await processFile(selected as string);
    } catch (err) {
      setError(String(err));
      setStage('error');
    }
  }

  async function processFile(filePath: string) {
    setStage('importing');
    setProgress(0);
    setProgressMessage('Starting import...');
    setError(null);

    try {
      const importResult = await importGenome(filePath, (p: ImportProgress) => {
        setProgress(Math.round(p.progress * 100));
        setProgressMessage(p.message);
      });
      setProgress(100);
      setResult(importResult);
      setStage('complete');
      await loadGenomes();

      // Auto-start reference downloads after a brief pause
      setTimeout(() => startReferenceDownloads(importResult.genomeId), 800);
    } catch (err) {
      setError(String(err));
      setStage('error');
    }
  }

  async function startReferenceDownloads(genomeId: number) {
    // Check which refs are already downloaded
    let alreadyReady: string[] = [];
    try {
      const statuses = await getReferenceStatus();
      alreadyReady = statuses.filter((s) => s.status === 'ready').map((s) => s.source);
    } catch {
      // ignore — will try downloading all
    }

    const toDownload = REF_DATABASES.filter((db) => !alreadyReady.includes(db.source));
    if (toDownload.length === 0) {
      setStage('all_done');
      return;
    }

    setStage('downloading_refs');
    const completed: string[] = [...alreadyReady];
    const errors: string[] = [];

    for (const db of toDownload) {
      setRefCurrent(db.label);
      setRefProgress(0);
      setRefMessage(`Downloading ${db.label}...`);

      try {
        await downloadReferenceDatabase(db.source, genomeId, (p: ReferenceProgress) => {
          setRefProgress(p.progress);
          setRefMessage(p.message);
        });
        completed.push(db.source);
        setRefCompleted([...completed]);
      } catch {
        errors.push(db.label);
        setRefErrors([...errors]);
      }
    }

    setStage('all_done');
  }

  const totalRefSteps = REF_DATABASES.length;
  const completedRefSteps = refCompleted.length;

  return (
    <div className="max-w-2xl mx-auto">
      <SectionHeader
        title="Import Genome Data"
        description="Upload your raw DNA data file from 23andMe, AncestryDNA, or other direct-to-consumer genetic testing services."
      />

      <AnimatePresence mode="wait">
        {stage === 'idle' && (
          <motion.div key="idle" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.2 }}>
            <div
              ref={dropRef}
              className={`border-2 border-dashed rounded-sm p-16 text-center transition-colors duration-150 ${
                isDragging ? 'border-accent bg-accent/5' : 'border-border hover:border-text-muted'
              }`}
            >
              <p className="text-sm text-text-muted mb-1">Drop your 23andMe raw data file</p>
              <p className="text-xs text-text-muted mb-6">Supported formats: .txt, .csv, .tsv</p>
              <button onClick={handleFileSelect} className="px-5 py-2 text-sm border border-border rounded-sm text-text hover:border-accent hover:text-accent transition-colors duration-150">
                Or choose file
              </button>
            </div>
            {(dropError || error) && <p className="mt-3 text-xs text-risk-high">{dropError || error}</p>}
            <p className="mt-8 text-xs text-text-muted">Your genetic data never leaves this device. All processing happens locally.</p>
          </motion.div>
        )}

        {stage === 'importing' && (
          <motion.div key="importing" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.2 }}>
            <Card>
              <div className="py-6">
                <div className="flex items-center gap-3 mb-4">
                  <StepDot active />
                  <p className="text-sm text-text">Importing genome data...</p>
                </div>
                <div className="ml-6">
                  <ProgressBar progress={progress / 100} />
                  <p className="text-xs text-text-muted font-mono mt-1.5">{progress}% — {progressMessage}</p>
                </div>
              </div>
            </Card>
          </motion.div>
        )}

        {(stage === 'complete' || stage === 'downloading_refs') && result && (
          <motion.div key="complete" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.2 }}>
            <Card>
              <div className="py-4">
                {/* Step 1: Import — done */}
                <div className="flex items-center gap-3 mb-3">
                  <StepDot done />
                  <p className="text-sm text-text">
                    Imported <span className="font-mono font-semibold">{formatNumber(result.snpCount)}</span> variants
                    <span className="text-text-muted"> · {result.format} · Build {result.build ?? 'Unknown'} · {formatPercentage(result.qualitySummary.skipRate)} skipped</span>
                  </p>
                </div>

                {/* Step 2: Reference databases */}
                {stage === 'downloading_refs' && (
                  <div className="ml-6 mt-4 border-t border-border pt-4">
                    <div className="flex items-center gap-3 mb-3">
                      <StepDot active />
                      <p className="text-sm text-text">Downloading reference databases...</p>
                    </div>
                    <div className="ml-6 space-y-3">
                      {REF_DATABASES.map((db) => {
                        const isDone = refCompleted.includes(db.source);
                        const isCurrent = refCurrent === db.label && !isDone;
                        const isFailed = refErrors.includes(db.label);
                        return (
                          <div key={db.source}>
                            <div className="flex items-center gap-2 text-xs">
                              {isDone && <span className="text-risk-low">&#10003;</span>}
                              {isFailed && <span className="text-risk-high">&#10007;</span>}
                              {isCurrent && <span className="text-accent animate-pulse">&#9679;</span>}
                              {!isDone && !isCurrent && !isFailed && <span className="text-text-muted">&#9675;</span>}
                              <span className={isDone ? 'text-text' : isCurrent ? 'text-accent' : 'text-text-muted'}>{db.label}</span>
                            </div>
                            {isCurrent && (
                              <div className="ml-5 mt-1">
                                <ProgressBar progress={refProgress} />
                                <p className="text-[10px] text-text-muted font-mono mt-1">{refMessage}</p>
                              </div>
                            )}
                          </div>
                        );
                      })}
                      <p className="text-[10px] text-text-muted mt-2">
                        {completedRefSteps}/{totalRefSteps} databases loaded
                      </p>
                    </div>
                  </div>
                )}

                {stage === 'complete' && (
                  <div className="ml-6 mt-3">
                    <div className="flex items-center gap-2 text-xs text-text-muted">
                      <span className="animate-pulse">&#9679;</span>
                      <span>Preparing reference databases...</span>
                    </div>
                  </div>
                )}
              </div>
            </Card>
          </motion.div>
        )}

        {stage === 'all_done' && result && (
          <motion.div key="all_done" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.2 }}>
            <Card>
              <div className="py-4">
                <div className="flex items-center gap-3 mb-2">
                  <StepDot done />
                  <p className="text-sm text-text">
                    <span className="font-mono font-semibold">{formatNumber(result.snpCount)}</span> variants imported
                  </p>
                </div>
                <div className="flex items-center gap-3 mb-4">
                  <StepDot done />
                  <p className="text-sm text-text">
                    Reference databases loaded
                    {refErrors.length > 0 && (
                      <span className="text-text-muted"> ({refErrors.length} failed — can retry in Settings)</span>
                    )}
                  </p>
                </div>

                <p className="text-sm text-text-muted mb-4">
                  Your genome is ready. Ask questions, explore findings, or browse the research feed.
                </p>

                <div className="flex gap-3">
                  <button onClick={() => navigate('/ask')} className="px-5 py-2 text-sm bg-accent text-white rounded-sm hover:bg-accent/90 transition-colors duration-150">
                    Ask About Your Genome
                  </button>
                  <button onClick={() => navigate('/')} className="px-5 py-2 text-sm border border-border rounded-sm text-text-muted hover:text-text transition-colors duration-150">
                    Dashboard
                  </button>
                </div>
              </div>
            </Card>
          </motion.div>
        )}

        {stage === 'error' && (
          <motion.div key="error" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }} transition={{ duration: 0.2 }}>
            <Card>
              <div className="py-4 text-center">
                <p className="text-sm text-risk-high mb-3">Import failed</p>
                <p className="text-xs text-text-muted mb-4">{error}</p>
                <button
                  onClick={() => { setStage('idle'); setError(null); }}
                  className="px-5 py-2 text-sm border border-border rounded-sm text-text hover:border-accent transition-colors duration-150"
                >
                  Try Again
                </button>
              </div>
            </Card>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function StepDot({ active, done }: { active?: boolean; done?: boolean }) {
  if (done) {
    return <span className="w-4 h-4 rounded-full bg-risk-low text-white text-[9px] flex items-center justify-center shrink-0">&#10003;</span>;
  }
  if (active) {
    return <span className="w-4 h-4 rounded-full bg-accent animate-pulse shrink-0" />;
  }
  return <span className="w-4 h-4 rounded-full border border-border shrink-0" />;
}

function ProgressBar({ progress }: { progress: number }) {
  return (
    <div className="w-full h-1 bg-border rounded-full overflow-hidden">
      <motion.div
        className="h-full bg-accent rounded-full"
        initial={{ width: 0 }}
        animate={{ width: `${Math.round(progress * 100)}%` }}
        transition={{ duration: 0.3 }}
      />
    </div>
  );
}
