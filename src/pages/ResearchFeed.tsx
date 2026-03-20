import { useState, useEffect, useCallback } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import {
  scanResearch,
  getResearchDigest,
} from '../lib/tauri-bridge';
import type { DigestItem, ResearchDigest, ScanProgress } from '../lib/tauri-bridge';
import { formatDate } from '../lib/formatters';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

function RelevanceBar({ score }: { score: number }) {
  const totalBars = 10;
  const filledBars = Math.round(score * totalBars);
  return (
    <span className="font-mono text-[10px] tracking-tight">
      {Array.from({ length: totalBars }, (_, i) => (
        <span
          key={i}
          className={i < filledBars ? 'text-accent' : 'text-border'}
        >
          {i < filledBars ? '\u2588' : '\u2591'}
        </span>
      ))}
      <span className="ml-1.5 text-text-muted">{score.toFixed(2)}</span>
    </span>
  );
}

function formatRelativeTime(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return 'just now';
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDays = Math.floor(diffHr / 24);
    if (diffDays === 1) return 'yesterday';
    if (diffDays < 30) return `${diffDays}d ago`;
    return formatDate(dateStr);
  } catch {
    return dateStr;
  }
}

function DigestItemCard({ item }: { item: DigestItem }) {
  return (
    <Card className="mb-3">
      <div className="flex items-start gap-4">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            {item.isNew && (
              <span className="inline-flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wider text-emerald-600">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-500 inline-block" />
                New
              </span>
            )}
            <p className="text-sm font-medium text-text leading-snug">{item.title}</p>
          </div>

          <div className="flex items-center gap-1.5 mb-2 text-[10px] text-text-muted">
            {item.authors && <span>{item.authors}</span>}
            {item.authors && item.journal && <span>&middot;</span>}
            {item.journal && <span>{item.journal}</span>}
            {item.publishedDate && <span>&middot;</span>}
            {item.publishedDate && <span>{item.publishedDate}</span>}
          </div>

          {item.summary && (
            <p className="text-xs text-text-muted leading-relaxed mb-2 line-clamp-2">
              {item.summary}
            </p>
          )}

          <div className="flex items-center gap-2 flex-wrap mb-2">
            {item.matchedRsids.map((rsid) => (
              <span
                key={rsid}
                className="text-[10px] font-mono text-accent bg-accent/5 px-1.5 py-0.5 rounded-sm"
              >
                {rsid}
              </span>
            ))}
          </div>

          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2">
              <span className="text-[10px] text-text-muted">Relevance</span>
              <RelevanceBar score={item.relevanceScore} />
            </div>
          </div>

          <div className="mt-2">
            <span className="text-[10px] text-text-muted font-mono select-all">
              {item.pubmedUrl}
            </span>
          </div>
        </div>
      </div>
    </Card>
  );
}

export default function ResearchFeed() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const [digest, setDigest] = useState<ResearchDigest | null>(null);
  const [scanning, setScanning] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const loadDigest = useCallback(async () => {
    if (!activeGenomeId) return;
    try {
      const result = await getResearchDigest(activeGenomeId);
      setDigest(result);
      setLoaded(true);
    } catch (err) {
      setError(String(err));
      setLoaded(true);
    }
  }, [activeGenomeId]);

  useEffect(() => {
    loadDigest();
  }, [loadDigest]);

  async function handleScan() {
    if (!activeGenomeId) return;
    setScanning(true);
    setScanProgress(null);
    setError(null);
    try {
      await scanResearch(activeGenomeId, (p) => {
        setScanProgress(p);
      });
      // Reload digest after scan
      await loadDigest();
    } catch (err) {
      setError(String(err));
    } finally {
      setScanning(false);
      setScanProgress(null);
    }
  }

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to view research.</p>
      </div>
    );
  }

  const lastScanLabel = digest?.lastScanDate
    ? formatRelativeTime(digest.lastScanDate)
    : null;

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Research Feed"
        description="Studies matched to your genome"
        action={
          <div className="flex items-center gap-3">
            {lastScanLabel && (
              <span className="text-[10px] text-text-muted">
                Last scan: {lastScanLabel}
              </span>
            )}
            <button
              onClick={handleScan}
              disabled={scanning}
              className="px-4 py-1.5 text-xs border border-border rounded-sm text-text hover:border-accent hover:text-accent transition-colors duration-100 disabled:opacity-50"
            >
              {scanning ? 'Scanning...' : 'Scan Now'}
            </button>
          </div>
        }
      />

      {/* Scan progress indicator */}
      {scanning && scanProgress && (
        <Card className="mb-4 p-4">
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs text-text-muted">{scanProgress.message}</span>
              <span className="text-[10px] font-mono text-text-muted">
                {Math.round(scanProgress.progress * 100)}%
              </span>
            </div>
            <div className="w-full h-0.5 bg-border rounded-full overflow-hidden">
              <div
                className="h-full bg-accent transition-all duration-300"
                style={{ width: `${scanProgress.progress * 100}%` }}
              />
            </div>
          </div>
        </Card>
      )}

      {/* New articles banner */}
      {digest && digest.totalNew > 0 && !scanning && (
        <Card className="mb-4 p-4 border-emerald-500/30">
          <p className="text-xs text-emerald-600 font-medium">
            {digest.totalNew} new {digest.totalNew === 1 ? 'study' : 'studies'} relevant to your genome since your last visit
          </p>
        </Card>
      )}

      {error && (
        <p className="text-xs text-risk-high mb-4">{error}</p>
      )}

      {loaded && digest && digest.items.length === 0 && !scanning && (
        <div className="text-center py-12">
          <p className="text-sm text-text-muted mb-2">
            No research articles found yet.
          </p>
          <p className="text-xs text-text-muted">
            Click "Scan Now" to search PubMed for studies matching your genetic variants.
          </p>
        </div>
      )}

      <div>
        {digest?.items.map((item) => (
          <DigestItemCard key={item.articleId} item={item} />
        ))}
      </div>

      {/* Privacy notice */}
      {loaded && (
        <div className="mt-8 pt-4 border-t border-border">
          <p className="text-[10px] text-text-muted leading-relaxed">
            Privacy: Only public rsID identifiers are searched. Your genotype data never leaves this device.
          </p>
        </div>
      )}
    </motion.div>
  );
}
