import React, { useState, useCallback, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useGenomeStore } from '../stores/genomeStore';
import { askGenome } from '../lib/tauri-bridge';
import type { GenomeAnswer, RelatedSnp } from '../lib/tauri-bridge';
import SectionHeader from '../design-system/components/SectionHeader';

interface ConversationEntry {
  question: string;
  answer: GenomeAnswer;
  timestamp: number;
}

const QUICK_QUESTIONS = [
  'Summarize my genome',
  'Am I at risk for diabetes?',
  'Eye color genetics',
  'Caffeine metabolism',
  'Am I a carrier for anything?',
  'How do I metabolize drugs?',
  "What's on chromosome 6?",
  'Tell me about APOE',
];

function renderMarkdown(text: string): React.JSX.Element[] {
  const lines = text.split('\n');
  const elements: React.JSX.Element[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    if (line.trim() === '---') {
      elements.push(<hr key={i} className="border-border my-3" />);
      continue;
    }

    if (line.trim() === '') {
      elements.push(<div key={i} className="h-2" />);
      continue;
    }

    const rendered = renderInline(line);

    if (line.startsWith('- ')) {
      elements.push(
        <div key={i} className="flex gap-2 ml-2 mb-1">
          <span className="text-text-muted shrink-0 mt-0.5">&bull;</span>
          <span>{rendered}</span>
        </div>,
      );
    } else {
      elements.push(
        <p key={i} className="mb-1">
          {rendered}
        </p>,
      );
    }
  }

  return elements;
}

function renderInline(text: string): (React.JSX.Element | string)[] {
  const parts: (React.JSX.Element | string)[] = [];
  let remaining = text;
  let keyIdx = 0;

  while (remaining.length > 0) {
    // Bold: **text**
    const boldMatch = remaining.match(/^(.*?)\*\*(.+?)\*\*(.*)/s);
    if (boldMatch) {
      if (boldMatch[1]) {
        parts.push(...renderItalics(boldMatch[1], keyIdx));
        keyIdx++;
      }
      parts.push(
        <strong key={`b-${keyIdx}`} className="font-semibold text-text">
          {boldMatch[2]}
        </strong>,
      );
      keyIdx++;
      remaining = boldMatch[3];
      continue;
    }

    // If no bold, check for italics then push rest
    parts.push(...renderItalics(remaining, keyIdx));
    break;
  }

  return parts;
}

function renderItalics(
  text: string,
  startKey: number,
): (React.JSX.Element | string)[] {
  const parts: (React.JSX.Element | string)[] = [];
  let remaining = text;
  let keyIdx = startKey;

  while (remaining.length > 0) {
    const italicMatch = remaining.match(/^(.*?)\*(.+?)\*(.*)/s);
    if (italicMatch) {
      if (italicMatch[1]) {
        parts.push(italicMatch[1]);
      }
      parts.push(
        <em key={`i-${keyIdx}`} className="italic text-text-muted">
          {italicMatch[2]}
        </em>,
      );
      keyIdx++;
      remaining = italicMatch[3];
      continue;
    }

    if (remaining) {
      parts.push(remaining);
    }
    break;
  }

  return parts;
}

function ConfidenceBadge({ confidence }: { confidence: string }) {
  const styles: Record<string, string> = {
    high: 'bg-green-500/10 text-green-400 border-green-500/20',
    moderate: 'bg-yellow-500/10 text-yellow-400 border-yellow-500/20',
    low: 'bg-zinc-500/10 text-zinc-400 border-zinc-500/20',
  };

  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 text-[10px] uppercase tracking-wider border rounded-sm ${styles[confidence] || styles.low}`}
    >
      {confidence} confidence
    </span>
  );
}

function SourceList({ sources }: { sources: GenomeAnswer['sources'] }) {
  if (sources.length === 0) return null;

  const labels: Record<string, string> = {
    your_genome: 'Your genome',
    clinvar: 'ClinVar',
    gwas: 'GWAS Catalog',
    snpedia: 'SNPedia',
    curated: 'Curated data',
  };

  return (
    <div className="flex items-center gap-2 text-[10px] text-text-muted mt-3">
      <span className="uppercase tracking-wider">Sources:</span>
      {sources.map((s, i) => (
        <span key={i} className="text-text-muted">
          {labels[s.sourceType] || s.sourceType}
          {i < sources.length - 1 ? ' · ' : ''}
        </span>
      ))}
    </div>
  );
}

function RelatedSnpList({ snps }: { snps: RelatedSnp[] }) {
  if (snps.length === 0) return null;

  return (
    <div className="mt-3 pt-3 border-t border-border">
      <p className="text-[10px] uppercase tracking-wider text-text-muted mb-2">
        Related Variants
      </p>
      <div className="flex flex-wrap gap-2">
        {snps.map((snp) => (
          <div
            key={snp.rsid}
            className="inline-flex items-center gap-1.5 px-2 py-1 bg-surface border border-border rounded-sm text-xs"
          >
            <span className="font-mono text-accent">{snp.rsid}</span>
            <span className="text-text-muted">{snp.genotype}</span>
            {snp.gene && (
              <span className="text-text-muted italic">{snp.gene}</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export default function Ask() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const [history, setHistory] = useState<ConversationEntry[]>([]);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = useCallback(
    async (questionText?: string) => {
      const q = (questionText ?? query).trim();
      if (!q || !activeGenomeId || loading) return;

      setLoading(true);
      setQuery('');

      try {
        const answer = await askGenome(q, activeGenomeId);
        setHistory((prev) => [
          { question: q, answer, timestamp: Date.now() },
          ...prev,
        ]);
      } catch (err) {
        const errorAnswer: GenomeAnswer = {
          question: q,
          answer: `**An error occurred while searching your genome.**\n\n${String(err)}`,
          sources: [],
          relatedSnps: [],
          confidence: 'low',
          disclaimer: '',
        };
        setHistory((prev) => [
          { question: q, answer: errorAnswer, timestamp: Date.now() },
          ...prev,
        ]);
      } finally {
        setLoading(false);
        inputRef.current?.focus();
      }
    },
    [query, activeGenomeId, loading],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleQuickQuestion = useCallback(
    (q: string) => {
      setQuery(q);
      handleSubmit(q);
    },
    [handleSubmit],
  );

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">
          Import genome data to start asking questions.
        </p>
      </div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.3 }}
      className="flex flex-col h-[calc(100vh-4rem)]"
    >
      <SectionHeader
        title="Ask About Your Genome"
        description="Ask questions about your variants, health risks, drug responses, traits, and more."
      />

      {/* Search input */}
      <div className="mb-4">
        <div className="flex gap-2">
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="What would you like to know about your genome?"
            disabled={loading}
            className="flex-1 px-4 py-3 text-sm border border-border rounded-sm bg-surface text-text placeholder:text-text-muted focus:outline-none focus:border-accent font-mono disabled:opacity-50 transition-colors"
          />
          <button
            onClick={() => handleSubmit()}
            disabled={loading || !query.trim()}
            className="px-5 py-3 text-sm border border-border rounded-sm text-text-muted hover:text-text hover:border-accent disabled:opacity-30 transition-colors duration-100 shrink-0"
          >
            {loading ? '...' : 'Ask'}
          </button>
        </div>
      </div>

      {/* Quick questions */}
      {history.length === 0 && (
        <div className="mb-6">
          <p className="text-[10px] uppercase tracking-wider text-text-muted mb-2">
            Quick questions
          </p>
          <div className="flex flex-wrap gap-2">
            {QUICK_QUESTIONS.map((q) => (
              <button
                key={q}
                onClick={() => handleQuickQuestion(q)}
                disabled={loading}
                className="px-3 py-1.5 text-xs border border-border rounded-sm text-text-muted hover:text-text hover:border-accent transition-colors duration-100 disabled:opacity-50"
              >
                {q}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Loading indicator */}
      <AnimatePresence>
        {loading && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            className="mb-4"
          >
            <div className="bg-surface border border-border rounded-sm p-5">
              <p className="text-xs text-text-muted animate-pulse">
                Searching your genome...
              </p>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Conversation history */}
      <div ref={resultsRef} className="flex-1 overflow-y-auto min-h-0 space-y-4 pb-8">
        <AnimatePresence>
          {history.map((entry, index) => (
            <motion.div
              key={entry.timestamp}
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.2, delay: index === 0 ? 0.1 : 0 }}
              className="bg-surface border border-border rounded-sm"
            >
              {/* Question */}
              <div className="px-5 py-3 border-b border-border">
                <p className="text-sm font-semibold text-text font-mono">
                  Q: {entry.question}
                </p>
              </div>

              {/* Answer */}
              <div className="px-5 py-4">
                <div className="text-sm text-text leading-relaxed">
                  {renderMarkdown(entry.answer.answer)}
                </div>

                <RelatedSnpList snps={entry.answer.relatedSnps} />

                <div className="flex items-center justify-between mt-3 pt-3 border-t border-border">
                  <SourceList sources={entry.answer.sources} />
                  <ConfidenceBadge confidence={entry.answer.confidence} />
                </div>

                {entry.answer.disclaimer && (
                  <p className="text-[10px] text-text-muted mt-3 leading-relaxed italic">
                    {entry.answer.disclaimer}
                  </p>
                )}
              </div>
            </motion.div>
          ))}
        </AnimatePresence>

        {history.length === 0 && !loading && (
          <div className="flex items-center justify-center py-16">
            <div className="text-center max-w-md">
              <p className="text-sm text-text-muted mb-4">
                Ask a question to explore your genomic data. You can ask about
                specific variants, genes, health risks, drug responses, traits,
                carrier status, or get a genome overview.
              </p>
              <div className="grid grid-cols-2 gap-3 text-xs text-text-muted">
                <div className="text-left">
                  <p className="font-semibold text-text mb-1">Try asking:</p>
                  <p>&quot;What is rs12913832?&quot;</p>
                  <p>&quot;Do I have MTHFR variants?&quot;</p>
                  <p>&quot;Am I lactose intolerant?&quot;</p>
                </div>
                <div className="text-left">
                  <p className="font-semibold text-text mb-1">Or explore:</p>
                  <p>&quot;Summarize my genome&quot;</p>
                  <p>&quot;Am I at risk for celiac?&quot;</p>
                  <p>&quot;How do I metabolize caffeine?&quot;</p>
                </div>
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Quick questions at bottom when there's history */}
      {history.length > 0 && (
        <div className="pt-3 border-t border-border shrink-0">
          <div className="flex flex-wrap gap-2">
            {QUICK_QUESTIONS.filter(
              (q) => !history.some((h) => h.question === q),
            )
              .slice(0, 4)
              .map((q) => (
                <button
                  key={q}
                  onClick={() => handleQuickQuestion(q)}
                  disabled={loading}
                  className="px-2.5 py-1 text-[10px] border border-border rounded-sm text-text-muted hover:text-text hover:border-accent transition-colors duration-100 disabled:opacity-50"
                >
                  {q}
                </button>
              ))}
          </div>
        </div>
      )}
    </motion.div>
  );
}
