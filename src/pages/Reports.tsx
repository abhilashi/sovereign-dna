import { useState } from 'react';
import { motion } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { generateReport } from '../lib/tauri-bridge';
import SectionHeader from '../design-system/components/SectionHeader';
import Card from '../design-system/components/Card';

export default function Reports() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);

  const [generating, setGenerating] = useState(false);
  const [resultPath, setResultPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleGenerate() {
    if (!activeGenomeId) return;
    setGenerating(true);
    setError(null);
    try {
      const filePath = await generateReport(activeGenomeId);
      setResultPath(filePath);
    } catch (err) {
      setError(String(err));
    } finally {
      setGenerating(false);
    }
  }

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Import genome data to generate reports.</p>
      </div>
    );
  }

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ duration: 0.3 }}>
      <SectionHeader
        title="Generate Report"
        description="Create a comprehensive report of your genetic analysis"
      />

      <div className="max-w-lg">
        <Card className="mb-6">
          <p className="text-xs text-text-muted leading-relaxed">
            The report includes all analysis sections: Health Risks, Pharmacogenomics,
            Traits, Ancestry, and Carrier Status.
          </p>
        </Card>

        <div className="flex items-center gap-4">
          <button
            onClick={handleGenerate}
            disabled={generating}
            className="px-5 py-2 text-sm bg-accent text-white rounded-sm hover:bg-accent/90 transition-colors duration-150 disabled:opacity-50"
          >
            {generating ? 'Generating...' : 'Generate Report'}
          </button>
        </div>

        {error && (
          <p className="text-xs text-risk-high mt-3">{error}</p>
        )}

        {resultPath && (
          <Card className="mt-6">
            <p className="text-sm font-medium text-text mb-2">Report Generated</p>
            <div className="space-y-1 text-xs text-text-muted">
              <p className="font-mono">{resultPath}</p>
            </div>
          </Card>
        )}
      </div>
    </motion.div>
  );
}
