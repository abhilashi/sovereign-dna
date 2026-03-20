import { useState, useEffect, useMemo } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { getGenomeSummary } from '../lib/tauri-bridge';
import type { GenomeSummary } from '../lib/tauri-bridge';
import { LAYER_COLORS } from '../lib/constants';
import { formatNumber } from '../lib/formatters';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';
import CompactKaryogram from '../visualizations/CompactKaryogram';
import type { KaryogramFinding } from '../visualizations/CompactKaryogram';
import { useKaryogramData } from '../hooks/useKaryogramData';

// ---- Component ----

export default function GenomeMap() {
  const { activeGenomeId, loadGenomes, genomes } = useGenomeStore();

  const [summary, setSummary] = useState<GenomeSummary | null>(null);
  const [highlightedFinding, setHighlightedFinding] = useState<KaryogramFinding | null>(null);
  const [expandedCategory, setExpandedCategory] = useState<string | null>(null);

  const {
    layout,
    findings,
    findingsWithPositions,
    loading,
  } = useKaryogramData(activeGenomeId ?? null);

  useEffect(() => { loadGenomes(); }, [loadGenomes]);

  // Load summary separately (not part of karyogram data)
  useEffect(() => {
    if (!activeGenomeId) return;
    getGenomeSummary(activeGenomeId).then(setSummary).catch(() => {});
  }, [activeGenomeId]);

  const findingsByCategory = useMemo(() => {
    const grouped = new Map<string, KaryogramFinding[]>();
    for (const f of findings) {
      const arr = grouped.get(f.category) || [];
      arr.push(f);
      grouped.set(f.category, arr);
    }
    return grouped;
  }, [findings]);

  if (!activeGenomeId || genomes.length === 0) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <div className="text-center">
          <p className="text-sm text-text-muted mb-2">No genome data loaded</p>
          <a href="/import" className="text-sm text-accent hover:underline">
            Import your genome data
          </a>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Analyzing your genome...</p>
      </div>
    );
  }

  const totalFindings = findings.length;
  const actionableFindings = findings.filter(
    (f) => f.category === 'health' || f.category === 'pharma' || f.category === 'carrier',
  ).length;

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.3 }}
      className="max-w-5xl"
    >
      {/* Header with key stats */}
      <SectionHeader
        title="Your Genome"
        description={
          summary
            ? `${formatNumber(summary.totalSnps)} variants analyzed · ${totalFindings} findings · ${actionableFindings} clinically relevant · Build ${summary.genome.build ?? 'Unknown'}`
            : undefined
        }
      />

      {/* Compact karyogram with findings plotted */}
      {layout && (
        <Card className="mb-6 p-5">
          <CompactKaryogram
            layout={layout}
            findings={findingsWithPositions}
            highlighted={highlightedFinding}
            onFindingClick={setHighlightedFinding}
          />
        </Card>
      )}

      {/* Findings by category */}
      <div className="space-y-6">
        <CategorySection
          category="health"
          label="Health Risks"
          description="Variants associated with disease risk from your genome"
          color={LAYER_COLORS.health}
          findings={findingsByCategory.get('health') || []}
          expanded={expandedCategory === 'health'}
          onToggle={() => setExpandedCategory(expandedCategory === 'health' ? null : 'health')}
          highlighted={highlightedFinding}
          onHighlight={setHighlightedFinding}
        />
        <CategorySection
          category="pharma"
          label="Drug Response"
          description="How your genes affect medication metabolism"
          color={LAYER_COLORS.pharma}
          findings={findingsByCategory.get('pharma') || []}
          expanded={expandedCategory === 'pharma'}
          onToggle={() => setExpandedCategory(expandedCategory === 'pharma' ? null : 'pharma')}
          highlighted={highlightedFinding}
          onHighlight={setHighlightedFinding}
        />
        <CategorySection
          category="traits"
          label="Traits"
          description="Physical and behavioral trait predictions"
          color={LAYER_COLORS.traits}
          findings={findingsByCategory.get('traits') || []}
          expanded={expandedCategory === 'traits'}
          onToggle={() => setExpandedCategory(expandedCategory === 'traits' ? null : 'traits')}
          highlighted={highlightedFinding}
          onHighlight={setHighlightedFinding}
        />
        <CategorySection
          category="carrier"
          label="Carrier Status"
          description="Recessive conditions you may carry"
          color={LAYER_COLORS.carrier}
          findings={findingsByCategory.get('carrier') || []}
          expanded={expandedCategory === 'carrier'}
          onToggle={() => setExpandedCategory(expandedCategory === 'carrier' ? null : 'carrier')}
          highlighted={highlightedFinding}
          onHighlight={setHighlightedFinding}
        />
      </div>

      {findings.length === 0 && (
        <Card className="p-8 text-center">
          <p className="text-sm text-text-muted">
            No findings yet. Your genome data has been imported — analysis results will appear
            here as reference databases are loaded.
          </p>
        </Card>
      )}

      {/* Footer */}
      <div className="mt-8 mb-4 text-xs text-text-muted leading-relaxed">
        <p>
          These results are for informational and educational purposes only. They are not
          medical diagnoses. Consult a healthcare provider or genetic counselor for clinical
          interpretation.
        </p>
      </div>
    </motion.div>
  );
}

// ---- Category Section ----

function CategorySection({
  category,
  label,
  description,
  color,
  findings,
  expanded,
  onToggle,
  highlighted,
  onHighlight,
}: {
  category: string;
  label: string;
  description: string;
  color: string;
  findings: KaryogramFinding[];
  expanded: boolean;
  onToggle: () => void;
  highlighted: KaryogramFinding | null;
  onHighlight: (f: KaryogramFinding | null) => void;
}) {
  void category; // used for semantic grouping only
  if (findings.length === 0) return null;

  const displayFindings = expanded ? findings : findings.slice(0, 3);
  const hasMore = findings.length > 3;

  return (
    <div>
      {/* Category header */}
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-3 group mb-2"
      >
        <span
          className="w-3 h-3 rounded-full shrink-0"
          style={{ backgroundColor: color }}
        />
        <span className="text-sm font-semibold text-text">{label}</span>
        <span className="text-xs text-text-muted">{findings.length}</span>
        <span className="text-xs text-text-muted ml-auto">{description}</span>
        <span className="text-xs text-text-muted ml-2">
          {expanded ? '\u25B4' : '\u25BE'}
        </span>
      </button>

      {/* Findings list */}
      <div className="space-y-1 ml-6">
        {displayFindings.map((finding) => {
          const isHighlighted = highlighted?.id === finding.id;
          return (
            <div
              key={finding.id}
              className={`flex items-start gap-3 py-2 px-3 rounded-sm cursor-pointer transition-colors duration-100 ${
                isHighlighted
                  ? 'bg-accent/5 border-l-2'
                  : 'hover:bg-surface border-l-2 border-transparent'
              }`}
              style={isHighlighted ? { borderLeftColor: color } : undefined}
              onMouseEnter={() => onHighlight(finding)}
              onMouseLeave={() => onHighlight(null)}
              onClick={() => onHighlight(isHighlighted ? null : finding)}
            >
              {/* Content */}
              <div className="flex-1 min-w-0">
                <div className="flex items-baseline gap-2">
                  <span className="text-sm font-medium text-text">{finding.title}</span>
                  <span className="text-xs text-text-muted">{finding.subtitle}</span>
                </div>
                <p className="text-xs text-text-muted mt-0.5 truncate">
                  {finding.detail}
                </p>
              </div>

              {/* Position badge */}
              {finding.chromosome && (
                <span className="text-[10px] text-text-muted font-mono shrink-0 mt-0.5">
                  chr{finding.chromosome}
                </span>
              )}

              {/* Significance badge */}
              <span
                className="text-[10px] px-1.5 py-0.5 rounded-sm shrink-0 mt-0.5"
                style={{
                  backgroundColor: color + '15',
                  color: color,
                }}
              >
                {finding.significance}
              </span>
            </div>
          );
        })}

        {hasMore && !expanded && (
          <button
            onClick={onToggle}
            className="text-xs text-accent hover:underline pl-3 py-1"
          >
            Show {findings.length - 3} more {label.toLowerCase()}
          </button>
        )}
      </div>
    </div>
  );
}
