import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { open as shellOpen } from '@tauri-apps/plugin-shell';
import { useGenomeStore } from '../stores/genomeStore';
import { useSettingsStore } from '../stores/settingsStore';
import {
  researchQuery, chatWithClaude, chatLocalLlm, checkLocalLlm,
  getWorkbenchSessions, saveWorkbenchChat,
} from '../lib/tauri-bridge';
import type { EvidenceSnp, WorkbenchArticle, LocalLlmStatus } from '../lib/tauri-bridge';
import CompactKaryogram from '../visualizations/CompactKaryogram';
import type { KaryogramFinding } from '../visualizations/CompactKaryogram';
import { useKaryogramData } from '../hooks/useKaryogramData';

// ── Types ────────────────────────────────────────────────────────

// A single conversation message — can be user text, local response, claude response,
// or system-injected evidence/articles blocks
type MessageType = 'user' | 'local' | 'claude' | 'evidence' | 'articles' | 'consent' | 'pipeline' | 'claude_offer';

interface ConvoMessage {
  id: string;
  type: MessageType;
  content: string;
  snps?: EvidenceSnp[];
  articles?: WorkbenchArticle[];
  timestamp: number;
}

interface Thread {
  id: string;
  title: string;
  timestamp: number;
  messages: ConvoMessage[];
  claudeContext: string | null;
  consentGiven: boolean;
}

// ── Constants ────────────────────────────────────────────────────

const PIPELINE_STEPS = ['Routing', 'Finding SNPs', 'Databases', 'PubMed', 'Complete'];
const PIPELINE_KEY_MAP: Record<string, string> = {
  routing: 'Routing', finding_snps: 'Finding SNPs', database_lookup: 'Databases', pubmed: 'PubMed', complete: 'Complete',
};
const QUICK_PROMPTS = [
  'Do I have cancer-related variants?',
  'Am I at risk for heart disease?',
  'Caffeine metabolism',
  'Carrier status overview',
  'Obesity risk',
  'Tell me about APOE',
];

// ── Helpers ──────────────────────────────────────────────────────

function openExternal(e: React.MouseEvent<HTMLAnchorElement>) {
  e.preventDefault();
  const href = e.currentTarget.getAttribute('href');
  if (href) shellOpen(href);
}

function renderMd(text: string): React.JSX.Element[] {
  const lines = text.split('\n');
  const els: React.JSX.Element[] = [];
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (l.trim() === '---') { els.push(<hr key={i} className="border-border my-2" />); continue; }
    if (l.trim() === '') { els.push(<div key={i} className="h-1" />); continue; }
    const r = inl(l);
    if (l.startsWith('- ')) els.push(<div key={i} className="flex gap-1.5 ml-2 mb-0.5"><span className="text-text-muted">&bull;</span><span>{r}</span></div>);
    else if (l.match(/^#{1,3}\s/)) els.push(<p key={i} className="font-semibold text-text mt-1 mb-0.5">{inl(l.replace(/^#{1,3}\s/, ''))}</p>);
    else els.push(<p key={i} className="mb-0.5">{r}</p>);
  }
  return els;
}

function inl(t: string): (React.JSX.Element | string)[] {
  const p: (React.JSX.Element | string)[] = [];
  let r = t, k = 0;
  while (r.length > 0) {
    const b = r.match(/^(.*?)\*\*(.+?)\*\*(.*)/s);
    if (b) { if (b[1]) p.push(b[1]); p.push(<strong key={k++} className="font-semibold">{b[2]}</strong>); r = b[3]; continue; }
    const i = r.match(/^(.*?)\*(.+?)\*(.*)/s);
    if (i) { if (i[1]) p.push(i[1]); p.push(<em key={k++} className="italic text-text-muted">{i[2]}</em>); r = i[3]; continue; }
    const c = r.match(/^(.*?)`(.+?)`(.*)/s);
    if (c) { if (c[1]) p.push(c[1]); p.push(<code key={k++} className="font-mono text-accent text-[0.85em]">{c[2]}</code>); r = c[3]; continue; }
    p.push(r); break;
  }
  return p;
}

// ── Evidence Card ────────────────────────────────────────────────

function EvidenceCard({ snp }: { snp: EvidenceSnp }) {
  return (
    <div className="border border-border rounded-sm p-2 mb-1 bg-white">
      <div className="flex items-baseline gap-1.5 flex-wrap">
        <a href={`https://www.ncbi.nlm.nih.gov/snp/${snp.rsid}`} className="font-mono text-accent text-[11px] hover:underline" onClick={openExternal}>{snp.rsid}{'\u2197'}</a>
        <span className="font-mono font-bold text-[11px]">{snp.genotype}</span>
        {snp.gene && <a href={`https://www.ncbi.nlm.nih.gov/gene/?term=${snp.gene}`} className="text-[10px] text-text-muted italic hover:underline" onClick={openExternal}>{snp.gene}{'\u2197'}</a>}
        <span className="text-[9px] text-text-muted ml-auto font-mono">chr{snp.chromosome}:{snp.position.toLocaleString()}</span>
      </div>
      {snp.clinvar && <div className="border-l-2 border-[#A94442] pl-1.5 mt-1"><a href={`https://www.ncbi.nlm.nih.gov/clinvar/?term=${snp.rsid}`} className="text-[9px] text-[#A94442] font-semibold uppercase hover:underline" onClick={openExternal}>ClinVar{'\u2197'}</a><p className="text-[10px] text-text">{snp.clinvar.clinicalSignificance} &mdash; {snp.clinvar.condition}</p></div>}
      {snp.gwas.length > 0 && <div className="border-l-2 border-[#2D5F8A] pl-1.5 mt-1"><p className="text-[9px] text-[#2D5F8A] font-semibold uppercase">GWAS</p>{snp.gwas.slice(0,2).map((g,i)=><p key={i} className="text-[10px] text-text">{g.traitName}{g.oddsRatio!=null&&` (OR: ${g.oddsRatio})`}</p>)}</div>}
      {snp.snpedia && <div className="border-l-2 border-[#4A7C59] pl-1.5 mt-1"><p className="text-[9px] text-[#4A7C59] font-semibold uppercase">SNPedia</p><p className="text-[10px] text-text">{snp.snpedia.summary}</p></div>}
    </div>
  );
}

function ArticleCard({ article }: { article: WorkbenchArticle }) {
  return (
    <div className="border border-border rounded-sm p-2 mb-1 bg-white">
      <p className="text-[11px] font-medium text-text leading-snug">{article.title}</p>
      <p className="text-[9px] text-text-muted mt-0.5">{article.authors} &middot; {article.journal} &middot; {article.publishedDate}</p>
      {article.matchedRsids.length > 0 && <div className="flex flex-wrap gap-0.5 mt-1">{article.matchedRsids.slice(0,4).map(r=><span key={r} className="px-1 py-0.5 text-[8px] font-mono text-accent bg-accent/5 border border-accent/20 rounded-sm">{r}</span>)}{article.matchedRsids.length>4&&<span className="text-[8px] text-text-muted">+{article.matchedRsids.length-4}</span>}</div>}
      <a href={`https://pubmed.ncbi.nlm.nih.gov/${article.pmid}/`} className="text-[9px] text-accent mt-0.5 font-mono hover:underline inline-block" onClick={openExternal}>PubMed {article.pmid}{'\u2197'}</a>
    </div>
  );
}

// ── Main ─────────────────────────────────────────────────────────

export default function ResearchWorkbench() {
  const activeGenomeId = useGenomeStore((s) => s.activeGenomeId);
  const loadGenomes = useGenomeStore((s) => s.loadGenomes);

  // Load genomes on mount
  useEffect(() => { loadGenomes(); }, [loadGenomes]);
  const claudeApiKey = useSettingsStore((s) => s.claudeApiKey);
  const setClaudeApiKey = useSettingsStore((s) => s.setClaudeApiKey);

  // Threads
  const [threads, setThreads] = useState<Thread[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);

  // Conversation
  const [messages, setMessages] = useState<ConvoMessage[]>([]);
  const [claudeContext, setClaudeContext] = useState<string | null>(null);
  const [consentGiven, setConsentGiven] = useState(false);
  const [currentSnps, setCurrentSnps] = useState<EvidenceSnp[]>([]);
  const [currentArticles, setCurrentArticles] = useState<WorkbenchArticle[]>([]);

  // Pipeline
  const [pipelineStep, setPipelineStep] = useState('');
  const [completedSteps, setCompletedSteps] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [pipelineElapsed, setPipelineElapsed] = useState<number | null>(null);

  // Claude tracking
  const [, setPendingClaudeQuery] = useState<string | null>(null);
  const [streamSource, setStreamSource] = useState<'local' | 'claude'>('local');

  // Streaming
  const [streaming, setStreaming] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const streamRef = useRef('');

  // Input
  const [query, setQuery] = useState('');
  const [apiKeyInput, setApiKeyInput] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [, setLocalLlmStatus] = useState<LocalLlmStatus | null>(null);

  // Karyogram
  const { layout: karyogramLayout, findingsWithPositions } = useKaryogramData(activeGenomeId ?? null);
  const karyogramFindings = useMemo((): KaryogramFinding[] => {
    if (currentSnps.length > 0) {
      return currentSnps.filter(s => s.chromosome && s.position).map(s => ({
        id: `ev-${s.rsid}`, category: (s.clinvar ? 'health' : s.gwas.length > 0 ? 'traits' : 'pharma') as 'health'|'pharma'|'traits'|'carrier',
        title: s.clinvar?.condition || s.gwas[0]?.traitName || s.gene || s.rsid,
        subtitle: s.gene || '', detail: '', chromosome: s.chromosome, position: s.position, rsid: s.rsid,
        significance: s.clinvar?.clinicalSignificance || 'variant',
        color: s.clinvar ? '#A94442' : s.gwas.length > 0 ? '#2D5F8A' : '#4A7C59',
      }));
    }
    return findingsWithPositions;
  }, [currentSnps, findingsWithPositions]);

  const chatEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const genomeId = activeGenomeId;

  // ── Effects ────────────────────────────────────────────────────

  useEffect(() => { checkLocalLlm().then(setLocalLlmStatus).catch(() => setLocalLlmStatus({ available: false, provider: 'none', model: null, fallback: 'structured_query' })); }, []);
  useEffect(() => { if (activeGenomeId) getWorkbenchSessions(activeGenomeId).then(sessions => { setThreads(sessions.map(s => ({ id: s.id, title: s.query, timestamp: new Date(s.createdAt).getTime(), messages: [], claudeContext: null, consentGiven: false }))); }).catch(() => {}); }, [activeGenomeId]);
  useEffect(() => { chatEndRef.current?.scrollIntoView({ behavior: 'smooth' }); }, [messages, streamingText]);
  useEffect(() => { inputRef.current?.focus(); }, []);

  const addMessage = useCallback((type: MessageType, content: string, snps?: EvidenceSnp[], articles?: WorkbenchArticle[]) => {
    setMessages(prev => [...prev, { id: `msg-${Date.now()}-${Math.random()}`, type, content, snps, articles, timestamp: Date.now() }]);
  }, []);

  // ── Thread management ──────────────────────────────────────────

  const startNewThread = useCallback(() => {
    if (activeThreadId && messages.length > 0) {
      setThreads(prev => prev.map(t => t.id === activeThreadId ? { ...t, messages, claudeContext, consentGiven } : t));
    }
    setActiveThreadId(null);
    setMessages([]); setClaudeContext(null); setConsentGiven(false);
    setCurrentSnps([]); setCurrentArticles([]);
    setCompletedSteps([]); setPipelineStep(''); setPipelineElapsed(null);
    setQuery(''); setError(null); setStreamSource('local');
    inputRef.current?.focus();
  }, [activeThreadId, messages, claudeContext, consentGiven]);

  const restoreThread = useCallback((thread: Thread) => {
    if (activeThreadId && messages.length > 0) {
      setThreads(prev => prev.map(t => t.id === activeThreadId ? { ...t, messages, claudeContext, consentGiven } : t));
    }
    setActiveThreadId(thread.id); setMessages(thread.messages);
    setClaudeContext(thread.claudeContext); setConsentGiven(thread.consentGiven);
    // Restore SNPs/articles from messages
    const evMsg = [...thread.messages].reverse().find(m => m.type === 'evidence');
    const arMsg = [...thread.messages].reverse().find(m => m.type === 'articles');
    setCurrentSnps(evMsg?.snps || []); setCurrentArticles(arMsg?.articles || []);
    setCompletedSteps(['Routing','Finding SNPs','Databases','PubMed','Complete']); setPipelineStep('Complete');
    setError(null);
  }, [activeThreadId, messages, claudeContext, consentGiven]);

  // ── Pipeline ───────────────────────────────────────────────────

  const trackStep = useCallback((stepKey: string) => {
    const label = PIPELINE_KEY_MAP[stepKey] || stepKey;
    setPipelineStep(label);
    setCompletedSteps(prev => {
      const idx = PIPELINE_STEPS.indexOf(label);
      if (idx < 0) return prev;
      const next = [...prev];
      for (let i = 0; i < idx; i++) if (!next.includes(PIPELINE_STEPS[i])) next.push(PIPELINE_STEPS[i]);
      if (stepKey === 'complete' && !next.includes('Complete')) next.push('Complete');
      return next;
    });
  }, []);

  // ── Submit handler ─────────────────────────────────────────────

  // Ask Claude with explicit approval
  const askClaude = useCallback(async (q: string) => {
    if (!claudeApiKey || !claudeContext) return;
    setPendingClaudeQuery(null);

    addMessage('consent', `Sharing ${currentSnps.length} variants + ${currentArticles.length} articles with Claude. Full genome stays on device.`);
    setStreamSource('claude');

    const allMsgs = messages.filter(m => m.type === 'user' || m.type === 'claude').map(m => ({ role: m.type === 'user' ? 'user' : 'assistant', content: m.content }));
    allMsgs.push({ role: 'user', content: q });

    setStreaming(true); setStreamingText(''); streamRef.current = '';
    try {
      await chatWithClaude(claudeApiKey, allMsgs, claudeContext, ev => {
        if ((ev.eventType === 'delta' || ev.eventType === 'text_delta') && ev.text) { streamRef.current += ev.text; setStreamingText(streamRef.current); }
      });
    } catch (err) { streamRef.current += `\n\n*Error: ${String(err)}*`; setStreamingText(streamRef.current); }
    addMessage('claude', streamRef.current);
    setStreamingText(''); setStreaming(false);

    if (activeThreadId) {
      saveWorkbenchChat(activeThreadId, 'user', q).catch(() => {});
      saveWorkbenchChat(activeThreadId, 'assistant', streamRef.current).catch(() => {});
    }
  }, [claudeApiKey, claudeContext, messages, currentSnps, currentArticles, addMessage, activeThreadId]);

  const handleSubmit = useCallback(async () => {
    const q = query.trim();
    if (!q || !genomeId || loading || streaming) return;
    setQuery('');

    let threadId = activeThreadId;
    if (!threadId) {
      threadId = `thread-${Date.now()}`;
      setThreads(prev => [{ id: threadId!, title: q, timestamp: Date.now(), messages: [], claudeContext: null, consentGiven: false }, ...prev]);
      setActiveThreadId(threadId);
    }

    addMessage('user', q);

    // Always local first
    setLoading(true); setError(null);
    setCompletedSteps([]); setPipelineStep('Routing'); setPipelineElapsed(null);
    const startTime = Date.now();

    try {
      const res = await researchQuery(q, genomeId, p => {
        trackStep(p.step);
        if (p.partialSnps) setCurrentSnps(p.partialSnps);
        if (p.partialArticles) setCurrentArticles(p.partialArticles);
      });
      setCurrentSnps(res.evidenceSnps); setCurrentArticles(res.articles);
      setClaudeContext(res.claudeContext);
      trackStep('complete'); setPipelineElapsed(Date.now() - startTime);

      const hasSnps = res.evidenceSnps.length > 0;
      const hasArticles = res.articles.length > 0;

      if (hasSnps) addMessage('evidence', `${res.evidenceSnps.length} variants found`, res.evidenceSnps);
      if (hasArticles) addMessage('articles', `${res.articles.length} articles found`, undefined, res.articles);

      if (!hasSnps && !hasArticles) {
        // No results — be honest, don't hallucinate
        addMessage('local', `**No matching variants or research found for "${q}".**\n\nYour genome was searched but no variants matched this query. This could mean:\n- No known genetic associations exist for this topic in our reference databases\n- The variants in your genome for this area are all common/benign\n- Try rephrasing with a specific gene name, rsID, or condition\n\nExamples: "BRCA1 variants", "rs1801133", "celiac disease risk"`);
      } else {
        // Results found — get local LLM analysis
        setStreaming(true); setStreamingText(''); streamRef.current = '';
        setStreamSource('local');
        try {
          await chatLocalLlm([{ role: 'user', content: q }], res.claudeContext, genomeId, ev => {
            if (ev.eventType === 'text_delta' && ev.text) { streamRef.current += ev.text; setStreamingText(streamRef.current); }
          });
        } catch (err) { streamRef.current += `\n\n*Error: ${String(err)}*`; setStreamingText(streamRef.current); }
        addMessage('local', streamRef.current);
        setStreamingText(''); setStreaming(false);

        // Offer Claude analysis inline (user must approve each time)
        if (claudeApiKey) {
          addMessage('claude_offer', q);
          setPendingClaudeQuery(q);
        }
      }

      setThreads(prev => prev.map(t => t.id === threadId ? { ...t, claudeContext: res.claudeContext } : t));
    } catch (err) { setError(String(err)); } finally { setLoading(false); }
  }, [query, genomeId, loading, streaming, activeThreadId, claudeApiKey, addMessage, trackStep]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSubmit(); } }, [handleSubmit]);

  const handleKaryogramClick = useCallback((finding: KaryogramFinding) => {
    const parts = [finding.rsid, finding.subtitle, finding.title].filter(Boolean);
    setQuery(`Tell me about ${parts.join(' - ')}`);
    setStreamSource('local');
    // Don't auto-submit — let user review/edit the query
  }, []);

  const handleSaveApiKey = useCallback(() => { if (apiKeyInput.trim()) { setClaudeApiKey(apiKeyInput.trim()); setApiKeyInput(''); } }, [apiKeyInput, setClaudeApiKey]);

  if (!activeGenomeId) {
    return (
      <div className="flex items-center justify-center h-screen">
        <div className="text-center">
          <p className="text-sm text-text mb-2">No genome data loaded</p>
          <p className="text-xs text-text-muted mb-4">Import your 23andMe raw data to start researching.</p>
          <a href="/import" className="px-4 py-2 text-sm bg-accent text-white rounded-sm hover:bg-accent/90 transition-colors inline-block">
            Import Genome Data
          </a>
        </div>
      </div>
    );
  }

  const showPipeline = completedSteps.length > 0 || pipelineStep !== '';

  // ── Render ─────────────────────────────────────────────────────

  return (
    <div className="flex h-screen">
      {/* Thread sidebar — full height */}
      <div className="w-[200px] shrink-0 border-r border-border flex flex-col bg-surface">
          {/* New thread button */}
          <div className="px-3 py-2 border-b border-border">
            <button onClick={startNewThread} className="w-full text-left px-2 py-1.5 text-[11px] border border-border rounded-sm text-text-muted hover:text-accent hover:border-accent transition-colors">
              + New thread
            </button>
          </div>

          {/* Thread list */}
          <div className="flex-1 overflow-y-auto">
            {threads.map(t => (
              <button key={t.id} onClick={() => restoreThread(t)}
                className={`w-full text-left px-3 py-2 border-b border-border transition-colors ${t.id === activeThreadId ? 'bg-accent/5 border-l-2 border-l-accent' : 'hover:bg-surface/80 border-l-2 border-l-transparent'}`}>
                <p className="text-[11px] text-text truncate">{t.title}</p>
              </button>
            ))}
          </div>

          {/* API key input (if not set) */}
          {!claudeApiKey && (
            <div className="px-3 py-2 border-t border-border">
              <div className="flex gap-1">
                <input type="password" value={apiKeyInput} onChange={e => setApiKeyInput(e.target.value)} onKeyDown={e => { if (e.key === 'Enter') handleSaveApiKey(); }}
                  placeholder="Claude API key" className="flex-1 min-w-0 px-1.5 py-1 text-[9px] border border-border rounded-sm bg-white text-text focus:outline-none focus:border-accent font-mono" />
                <button onClick={handleSaveApiKey} disabled={!apiKeyInput.trim()} className="px-1.5 py-1 text-[9px] border border-border rounded-sm text-text-muted hover:text-accent disabled:opacity-30 transition-colors shrink-0">OK</button>
              </div>
            </div>
          )}

          {/* Nav links at bottom */}
          <div className="border-t border-border px-2 py-2 space-y-0.5">
            {[
              { label: 'Genome Map', path: '/map' },
              { label: 'Dashboard', path: '/dashboard' },
              { label: 'Import', path: '/import' },
              { label: 'Settings', path: '/settings' },
            ].map(item => (
              <a key={item.path} href={item.path}
                className="block px-2 py-1 text-[10px] text-text-muted hover:text-text transition-colors rounded-sm hover:bg-border/30">
                {item.label}
              </a>
            ))}
            <p className="text-[8px] text-text-muted mt-1 px-2">Genome Studio &middot; Local only</p>
          </div>
        </div>

      {/* Right side: pipeline + conversation + karyogram + input */}
      <div className="flex-1 flex flex-col min-w-0">
        {/* Pipeline */}
        {showPipeline && (
          <div className="px-4 py-1.5 border-b border-border flex items-center gap-1 shrink-0">
            {PIPELINE_STEPS.map((step, idx) => {
              const done = completedSteps.includes(step);
              const active = step === pipelineStep && !done;
              return (
                <React.Fragment key={step}>
                  {idx > 0 && <div className={`flex-1 h-px ${done ? 'bg-accent' : 'bg-border'}`} />}
                  <div className="flex items-center gap-1">
                    <div className={`w-2 h-2 rounded-full ${done ? 'bg-accent' : active ? 'bg-accent/40 animate-pulse' : 'bg-border'}`} />
                    <span className={`text-[9px] ${active ? 'text-text' : 'text-text-muted'}`}>{step}</span>
                  </div>
                </React.Fragment>
              );
            })}
            {pipelineElapsed != null && completedSteps.includes('Complete') && <span className="text-[9px] text-text-muted ml-2">{(pipelineElapsed / 1000).toFixed(1)}s</span>}
          </div>
        )}

        {error && <div className="px-4 py-1 shrink-0"><p className="text-[10px] text-risk-high">{error}</p></div>}

        {/* Conversation stream */}
        <div className="flex-1 overflow-y-auto px-6 py-4 space-y-3">
            {/* Empty state */}
            {messages.length === 0 && !streaming && (
              <div className="flex items-center justify-center h-full">
                <div className="text-center max-w-md">
                  <p className="text-sm text-text mb-1">What would you like to know about your genome?</p>
                  <p className="text-xs text-text-muted mb-4">Ask a question and we'll find your relevant variants, match them to research, and give you an analysis.</p>
                  <div className="flex flex-wrap justify-center gap-1.5">
                    {QUICK_PROMPTS.map(p => (
                      <button key={p} onClick={() => { setQuery(p); }} disabled={loading}
                        className="px-2.5 py-1 text-[10px] border border-border rounded-sm text-text-muted hover:text-accent hover:border-accent disabled:opacity-50 transition-colors">{p}</button>
                    ))}
                  </div>
                </div>
              </div>
            )}

            {/* Messages */}
            {messages.map(msg => (
              <div key={msg.id}>
                {msg.type === 'user' && (
                  <div className="flex justify-end mb-1">
                    <div className="bg-accent/5 border border-accent/20 rounded-sm px-3 py-2 max-w-[70%]">
                      <p className="text-xs text-text">{msg.content}</p>
                    </div>
                  </div>
                )}

                {msg.type === 'evidence' && msg.snps && (
                  <div className="mb-2">
                    <p className="text-[9px] uppercase tracking-wider text-text-muted mb-1 flex items-center gap-1.5">
                      <span className="w-1.5 h-1.5 rounded-full bg-risk-low" />
                      Your Variants ({msg.snps.length})
                    </p>
                    <div className="grid grid-cols-2 gap-1">
                      <AnimatePresence>{msg.snps.slice(0, 6).map(s => <motion.div key={s.rsid} initial={{opacity:0,y:4}} animate={{opacity:1,y:0}}><EvidenceCard snp={s} /></motion.div>)}</AnimatePresence>
                    </div>
                    {msg.snps.length > 6 && <p className="text-[9px] text-text-muted mt-1">+ {msg.snps.length - 6} more variants</p>}
                  </div>
                )}

                {msg.type === 'articles' && msg.articles && (
                  <div className="mb-2">
                    <p className="text-[9px] uppercase tracking-wider text-text-muted mb-1 flex items-center gap-1.5">
                      <span className="w-1.5 h-1.5 rounded-full bg-[#2D5F8A]" />
                      Research ({msg.articles.length} articles)
                    </p>
                    <div className="grid grid-cols-2 gap-1">
                      <AnimatePresence>{msg.articles.slice(0, 4).map(a => <motion.div key={a.pmid} initial={{opacity:0,y:4}} animate={{opacity:1,y:0}}><ArticleCard article={a} /></motion.div>)}</AnimatePresence>
                    </div>
                    {msg.articles.length > 4 && <p className="text-[9px] text-text-muted mt-1">+ {msg.articles.length - 4} more articles</p>}
                  </div>
                )}

                {msg.type === 'local' && (
                  <div className="mb-2">
                    <p className="text-[9px] text-text-muted mb-0.5 flex items-center gap-1">
                      <span className="w-1.5 h-1.5 rounded-full bg-risk-low" />
                      Local Analysis
                    </p>
                    <div className="text-xs text-text leading-relaxed pl-3 border-l-2 border-risk-low/30">
                      {renderMd(msg.content)}
                    </div>
                  </div>
                )}

                {msg.type === 'claude' && (
                  <div className="mb-2">
                    <p className="text-[9px] text-text-muted mb-0.5 flex items-center gap-1">
                      <span className="w-1.5 h-1.5 rounded-full bg-ancestry-ochre" />
                      Claude
                    </p>
                    <div className="text-xs text-text leading-relaxed pl-3 border-l-2 border-ancestry-ochre/30">
                      {renderMd(msg.content)}
                    </div>
                  </div>
                )}

                {msg.type === 'consent' && (
                  <div className="mb-2 px-3 py-2 border border-ancestry-ochre/30 bg-ancestry-ochre/5 rounded-sm">
                    <p className="text-[10px] text-text-muted">{msg.content}</p>
                  </div>
                )}

                {msg.type === 'claude_offer' && !streaming && (
                  <div className="mb-2 flex items-center gap-2">
                    <button
                      onClick={() => askClaude(msg.content)}
                      disabled={streaming || loading}
                      className="flex items-center gap-1.5 px-3 py-1.5 text-[10px] border border-ancestry-ochre/40 rounded-sm text-ancestry-ochre hover:bg-ancestry-ochre/5 transition-colors disabled:opacity-40"
                    >
                      <span className="w-1.5 h-1.5 rounded-full bg-ancestry-ochre" />
                      Ask Claude to analyze this
                    </button>
                    <span className="text-[9px] text-text-muted">
                      Will share {currentSnps.length} variants + {currentArticles.length} articles
                    </span>
                  </div>
                )}
              </div>
            ))}

            {/* Streaming */}
            {streaming && streamingText && (
              <div className="mb-2">
                <p className="text-[9px] text-text-muted mb-0.5 flex items-center gap-1">
                  <span className={`w-1.5 h-1.5 rounded-full ${streamSource === 'claude' ? 'bg-ancestry-ochre' : 'bg-risk-low'}`} />
                  {streamSource === 'claude' ? 'Claude' : 'Local Analysis'}
                </p>
                <div className={`text-xs text-text leading-relaxed pl-3 border-l-2 ${streamSource === 'claude' ? 'border-ancestry-ochre/30' : 'border-risk-low/30'}`}>
                  {renderMd(streamingText)}
                </div>
              </div>
            )}

            {(loading || streaming) && !streamingText && (
              <div className="flex items-center gap-1.5">
                <div className="w-1.5 h-1.5 rounded-full bg-accent animate-pulse" />
                <span className="text-[10px] text-text-muted">{loading ? 'Searching your genome...' : 'Thinking...'}</span>
              </div>
            )}

            <div ref={chatEndRef} />
          </div>

        {/* Karyogram */}
        {karyogramLayout && karyogramFindings.length > 0 && (
          <div className="border-t border-border shrink-0 px-4 py-1">
            <CompactKaryogram layout={karyogramLayout} findings={karyogramFindings} highlighted={null} onFindingClick={handleKaryogramClick} />
          </div>
        )}

        {/* Input bar */}
        <div className="border-t border-border px-4 py-2.5 flex items-center gap-2 shrink-0">
          <span className="w-4 h-4 rounded-full bg-risk-low flex items-center justify-center shrink-0" title="Processed locally">
            <span className="text-white text-[7px]">{'\uD83D\uDD12'}</span>
          </span>
          <input ref={inputRef} type="text" value={query} onChange={e => setQuery(e.target.value)} onKeyDown={handleKeyDown}
            placeholder="Ask about your genome..."
            disabled={loading || streaming}
            className="flex-1 text-sm bg-transparent outline-none text-text placeholder:text-text-muted disabled:opacity-50" />
          <button onClick={handleSubmit} disabled={loading || streaming || !query.trim()}
            aria-label="Send message"
            className="px-2.5 py-1 text-sm border border-border rounded-sm text-text-muted hover:text-accent hover:border-accent disabled:opacity-30 transition-colors">{'\u2192'}</button>
        </div>
      </div>
    </div>
  );
}
