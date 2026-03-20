import { invoke, Channel } from '@tauri-apps/api/core';

// ---- Type Definitions ----

// Matches Rust: db::queries::Genome
export interface Genome {
  id: number;
  filename: string;
  format: string;
  importedAt: string;
  snpCount: number;
  build: string | null;
}

// Matches Rust: commands::import::QualitySummary
export interface QualitySummary {
  totalLines: number;
  skippedLines: number;
  validSnps: number;
  skipRate: number;
}

// Matches Rust: commands::import::ImportResult
export interface ImportResult {
  genomeId: number;
  snpCount: number;
  format: string;
  build: string | null;
  qualitySummary: QualitySummary;
}

// Matches Rust: commands::genome::GenomeSummary
export interface GenomeSummary {
  genome: Genome;
  chromosomeCounts: ChromosomeCount[];
  heterozygosityRate: number;
  missingDataPercent: number;
  totalSnps: number;
}

// Matches Rust: db::queries::ChromosomeCount
export interface ChromosomeCount {
  chromosome: string;
  count: number;
}

// Matches Rust: db::queries::SnpRow
export interface SnpRow {
  id: number | null;
  genomeId: number;
  rsid: string;
  chromosome: string;
  position: number;
  genotype: string;
}

// Matches Rust: db::queries::AnnotatedSnp
export interface AnnotatedSnp {
  rsid: string;
  chromosome: string;
  position: number;
  genotype: string;
  gene: string | null;
  clinicalSignificance: string | null;
  condition: string | null;
  reviewStatus: string | null;
  alleleFrequency: number | null;
  source: string | null;
}

// Matches Rust: commands::explorer::SnpPage
export interface SnpPage {
  rows: SnpRow[];
  total: number;
  offset: number;
  limit: number;
}

// Matches Rust: commands::explorer::SnpDetail
export interface SnpDetail {
  snp: SnpRow;
  annotations: AnnotatedSnp[];
}

// Matches Rust: analysis::health_risk::ContributingSnp
export interface ContributingSnp {
  rsid: string;
  gene: string;
  genotype: string;
  effect: string;
  riskAllele: string;
}

// Matches Rust: analysis::health_risk::HealthRiskResult
export interface HealthRiskResult {
  category: string;
  condition: string;
  riskLevel: string;
  score: number;
  contributingSnps: ContributingSnp[];
  studyCount: number;
  confidence: string;
  source: string;
}

// Matches Rust: analysis::pharmacogenomics::DrugInfo
export interface DrugInfo {
  name: string;
  category: string;
  recommendation: string;
  evidenceLevel: string;
}

// Matches Rust: analysis::pharmacogenomics::PharmaResult
export interface PharmaResult {
  gene: string;
  starAllele: string;
  phenotype: string;
  affectedDrugs: DrugInfo[];
  clinicalActionability: string;
  source: string;
}

// Matches Rust: analysis::traits::TraitSnp
export interface TraitSnp {
  rsid: string;
  genotype: string;
  effect: string;
}

// Matches Rust: analysis::traits::TraitResult
export interface TraitResult {
  name: string;
  category: string;
  prediction: string;
  confidence: number;
  description: string;
  contributingSnps: TraitSnp[];
  populationFrequency: number | null;
  source: string;
}

// Matches Rust: analysis::ancestry::PopulationComponent
export interface PopulationComponent {
  name: string;
  percentage: number;
  color: string;
}

// Matches Rust: analysis::ancestry::AncestryResult
export interface AncestryResult {
  populations: PopulationComponent[];
  maternalHaplogroup: string | null;
  paternalHaplogroup: string | null;
}

// Matches Rust: analysis::carrier::CarrierVariant
export interface CarrierVariant {
  rsid: string;
  genotype: string;
  pathogenicAllele: string;
  isCarrier: boolean;
}

// Matches Rust: analysis::carrier::CarrierResult
export interface CarrierResult {
  condition: string;
  gene: string;
  status: string;
  variantsChecked: CarrierVariant[];
  inheritancePattern: string;
  description: string;
  source: string;
}

// Matches Rust: research::fetcher::ResearchArticle
export interface ResearchArticle {
  id: string;
  title: string;
  abstractText: string;
  source: string;
  publishedDate: string;
  relevantRsids: string[];
}

// Matches Rust: research::matcher::MatchedArticle
export interface MatchedArticle {
  article: ResearchArticle;
  matchedRsids: string[];
  relevanceScore: number;
}

// ---- Tauri Command Wrappers ----

export interface ImportProgress {
  phase: string;
  progress: number;
  message: string;
}

export async function importGenome(
  filePath: string,
  onProgress?: (progress: ImportProgress) => void,
): Promise<ImportResult> {
  const channel = new Channel<ImportProgress>();
  if (onProgress) {
    channel.onmessage = onProgress;
  }
  return invoke<ImportResult>('import_genome', { filePath, channel });
}

export async function listGenomes(): Promise<Genome[]> {
  return invoke<Genome[]>('list_genomes');
}

export async function getGenomeSummary(genomeId: number): Promise<GenomeSummary> {
  return invoke<GenomeSummary>('get_genome_summary', { genomeId });
}

export async function deleteGenome(genomeId: number): Promise<void> {
  return invoke<void>('delete_genome', { genomeId });
}

export async function getHealthRisks(genomeId: number): Promise<HealthRiskResult[]> {
  return invoke<HealthRiskResult[]>('get_health_risks', { genomeId });
}

export async function getPharmacogenomics(genomeId: number): Promise<PharmaResult[]> {
  return invoke<PharmaResult[]>('get_pharmacogenomics', { genomeId });
}

export async function getTraits(genomeId: number): Promise<TraitResult[]> {
  return invoke<TraitResult[]>('get_trait_predictions', { genomeId });
}

export async function getAncestry(genomeId: number): Promise<AncestryResult> {
  return invoke<AncestryResult>('get_ancestry_analysis', { genomeId });
}

export async function getCarrierStatus(genomeId: number): Promise<CarrierResult[]> {
  return invoke<CarrierResult[]>('get_carrier_status', { genomeId });
}

export async function fetchResearchArticles(genomeId: number): Promise<MatchedArticle[]> {
  return invoke<MatchedArticle[]>('fetch_research', { genomeId });
}

export async function getSnps(
  genomeId: number,
  offset: number,
  limit: number,
  search?: string | null,
  chromosome?: string | null,
): Promise<SnpPage> {
  return invoke<SnpPage>('get_snps', { genomeId, offset, limit, search, chromosome });
}

export async function getSnpDetail(genomeId: number, rsid: string): Promise<SnpDetail> {
  return invoke<SnpDetail>('get_snp_detail', { genomeId, rsid });
}

export async function exportSnps(genomeId: number, format: 'csv' | 'json', filter?: string): Promise<string> {
  return invoke<string>('export_snps', { genomeId, format, filter });
}

export async function generateReport(genomeId: number): Promise<string> {
  return invoke<string>('generate_report', { genomeId });
}

// ---- Genome Map Types ----

export interface DensityBin {
  binStart: number;
  binEnd: number;
  count: number;
}

export interface ChromosomeLayout {
  chromosome: string;
  snpCount: number;
  minPosition: number;
  maxPosition: number;
  densityBins: DensityBin[];
}

export interface GenomeLayout {
  chromosomes: ChromosomeLayout[];
  totalSnps: number;
  totalSpan: number;
}

export interface MapSnp {
  rsid: string;
  chromosome: string;
  position: number;
  genotype: string;
  gene: string | null;
  clinicalSignificance: string | null;
  condition: string | null;
  hasHealthRisk: boolean;
  hasPharma: boolean;
  hasTrait: boolean;
  hasCarrier: boolean;
}

export interface ChromosomeDensity {
  chromosome: string;
  bins: DensityBin[];
  totalSnps: number;
  minPosition: number;
  maxPosition: number;
}

export interface OverlayMarker {
  rsid: string;
  chromosome: string;
  position: number;
  layer: string;
  label: string;
  significance: string;
}

// ---- Genome Map Commands ----

export async function getGenomeLayout(genomeId: number): Promise<GenomeLayout> {
  return invoke<GenomeLayout>('get_genome_layout', { genomeId });
}

export async function getRegionSnps(
  genomeId: number,
  chromosome: string,
  start: number,
  end: number,
): Promise<MapSnp[]> {
  return invoke<MapSnp[]>('get_region_snps', { genomeId, chromosome, start, end });
}

export async function getChromosomeDensity(
  genomeId: number,
  chromosome: string,
  numBins: number,
): Promise<ChromosomeDensity> {
  return invoke<ChromosomeDensity>('get_chromosome_density', { genomeId, chromosome, numBins });
}

export async function getAnalysisOverlay(genomeId: number): Promise<OverlayMarker[]> {
  return invoke<OverlayMarker[]>('get_analysis_overlay', { genomeId });
}

// ---- Reference Database Types ----

export interface ReferenceDbStatus {
  source: string;
  status: string; // 'not_started' | 'downloading' | 'parsing' | 'ready' | 'error'
  downloadedAt: string | null;
  parsedAt: string | null;
  recordCount: number;
  fileSizeBytes: number;
  errorMessage: string | null;
  version: string | null;
}

export interface ReferenceProgress {
  source: string;
  phase: string;
  progress: number;
  message: string;
}

export interface ReferenceLoadResult {
  source: string;
  recordCount: number;
  durationSecs: number;
}

// ---- Reference Database Commands ----

export async function downloadReferenceDatabase(
  source: string,
  genomeId: number | null,
  onProgress?: (p: ReferenceProgress) => void,
): Promise<ReferenceLoadResult> {
  const channel = new Channel<ReferenceProgress>();
  if (onProgress) channel.onmessage = onProgress;
  return invoke<ReferenceLoadResult>('download_reference_database', { source, genomeId, channel });
}

export async function getReferenceStatus(): Promise<ReferenceDbStatus[]> {
  return invoke<ReferenceDbStatus[]>('get_reference_databases_status');
}

export async function deleteReferenceDatabase(source: string): Promise<void> {
  return invoke<void>('delete_reference_database', { source });
}

// ---- Research Digest Types ----

export interface DigestItem {
  articleId: string;
  title: string;
  authors: string;
  journal: string;
  publishedDate: string;
  matchedRsids: string[];
  relevanceScore: number;
  summary: string;
  pubmedUrl: string;
  isNew: boolean;
}

export interface ResearchDigest {
  items: DigestItem[];
  totalNew: number;
  lastScanDate: string | null;
  nextScanDate: string | null;
}

export interface ScanProgress {
  phase: string;
  progress: number;
  message: string;
}

export interface ScanResult {
  newArticles: number;
  matchedArticles: number;
  scanDate: string;
}

// ---- Research Digest Commands ----

export async function scanResearch(
  genomeId: number,
  onProgress?: (p: ScanProgress) => void,
): Promise<ScanResult> {
  const channel = new Channel<ScanProgress>();
  if (onProgress) channel.onmessage = onProgress;
  return invoke<ScanResult>('scan_research', { genomeId, channel });
}

export async function getResearchDigest(genomeId: number): Promise<ResearchDigest> {
  return invoke<ResearchDigest>('get_research_digest', { genomeId });
}

export async function getNewResearchCount(genomeId: number): Promise<number> {
  return invoke<number>('get_new_research_count', { genomeId });
}

// ---- Ask Genome Types ----

// Matches Rust: commands::ask::GenomeAnswer
export interface GenomeAnswer {
  question: string;
  answer: string;
  sources: AnswerSource[];
  relatedSnps: RelatedSnp[];
  confidence: string;
  disclaimer: string;
}

// Matches Rust: commands::ask::AnswerSource
export interface AnswerSource {
  sourceType: string;
  detail: string;
}

// Matches Rust: commands::ask::RelatedSnp
export interface RelatedSnp {
  rsid: string;
  chromosome: string;
  position: number;
  genotype: string;
  gene: string | null;
  significance: string | null;
}

// ---- Ask Genome Command ----

export async function askGenome(question: string, genomeId: number): Promise<GenomeAnswer> {
  return invoke<GenomeAnswer>('ask_genome', { question, genomeId });
}

// ---- Workbench Types ----

export interface EvidenceSnp {
  rsid: string;
  chromosome: string;
  position: number;
  genotype: string;
  gene: string | null;
  whySelected: string;
  clinvar: ClinvarEvidence | null;
  gwas: GwasEvidence[];
  snpedia: SnpediaEvidence | null;
}

export interface ClinvarEvidence {
  clinicalSignificance: string;
  condition: string;
  reviewStatus: string | null;
}

export interface GwasEvidence {
  traitName: string;
  pValue: number | null;
  oddsRatio: number | null;
  riskAllele: string | null;
}

export interface SnpediaEvidence {
  summary: string;
  magnitude: number | null;
  repute: string | null;
}

export interface WorkbenchArticle {
  pmid: string;
  title: string;
  authors: string;
  journal: string;
  publishedDate: string;
  url: string;
  matchedRsids: string[];
}

export interface WorkbenchProgress {
  step: string;
  progress: number;
  message: string;
  strategy: string | null;
  partialSnps: EvidenceSnp[] | null;
  partialArticles: WorkbenchArticle[] | null;
}

export interface WorkbenchResult {
  query: string;
  strategy: string;
  evidenceSnps: EvidenceSnp[];
  articles: WorkbenchArticle[];
  claudeContext: string;
}

export interface ChatStreamEvent {
  eventType: string;  // "text_delta" | "complete" | "error"
  text: string | null;
}

export interface WorkbenchSession {
  id: string;
  genomeId: number;
  query: string;
  strategy: string;
  resultJson: string;
  createdAt: string;
}

export interface ChatMessageRow {
  id: number;
  sessionId: string;
  role: string;
  content: string;
  createdAt: string;
}

// ---- Workbench Commands ----

export async function researchQuery(
  query: string,
  genomeId: number,
  onProgress?: (p: WorkbenchProgress) => void,
): Promise<WorkbenchResult> {
  const channel = new Channel<WorkbenchProgress>();
  if (onProgress) channel.onmessage = onProgress;
  return invoke<WorkbenchResult>('research_query', { query, genomeId, channel });
}

export async function chatWithClaude(
  apiKey: string,
  messages: { role: string; content: string }[],
  context: string,
  onEvent?: (e: ChatStreamEvent) => void,
): Promise<void> {
  const channel = new Channel<ChatStreamEvent>();
  if (onEvent) channel.onmessage = onEvent;
  return invoke<void>('chat_with_claude', { apiKey, messages, context, channel });
}

export async function getWorkbenchSessions(genomeId: number): Promise<WorkbenchSession[]> {
  return invoke<WorkbenchSession[]>('get_workbench_sessions', { genomeId });
}

export async function saveWorkbenchChat(sessionId: string, role: string, content: string): Promise<void> {
  return invoke<void>('save_workbench_chat', { sessionId, role, content });
}

export async function getWorkbenchChat(sessionId: string): Promise<ChatMessageRow[]> {
  return invoke<ChatMessageRow[]>('get_workbench_chat', { sessionId });
}

// ---- Local LLM Types ----

export interface LocalLlmStatus {
  available: boolean;
  provider: string;
  model: string | null;
  fallback: string;
}

// ---- Local LLM Commands ----

export async function checkLocalLlm(): Promise<LocalLlmStatus> {
  return invoke<LocalLlmStatus>('check_local_llm');
}

export async function chatLocalLlm(
  messages: { role: string; content: string }[],
  context: string,
  genomeId: number,
  onEvent?: (e: ChatStreamEvent) => void,
): Promise<void> {
  const channel = new Channel<ChatStreamEvent>();
  if (onEvent) channel.onmessage = onEvent;
  return invoke<void>('chat_local_llm', { messages, context, genomeId, channel });
}
