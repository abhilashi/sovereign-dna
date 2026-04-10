import {
  useRef,
  useEffect,
  useState,
  useCallback,
  useMemo,
  type MouseEvent as ReactMouseEvent,
  type WheelEvent as ReactWheelEvent,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import * as d3 from 'd3';
import { colors } from '../design-system/tokens';
import type {
  GenomeLayout,
  ChromosomeLayout,
  MapSnp,
  ChromosomeDensity,
  OverlayMarker,
} from '../lib/tauri-bridge';
import {
  getGenomeLayout,
  getRegionSnps,
  getChromosomeDensity,
  getAnalysisOverlay,
} from '../lib/tauri-bridge';
import { CHROMOSOMES } from '../lib/constants';

// ---- Types ----

type ZoomLevel = 'genome' | 'chromosome' | 'region' | 'snp';

interface GenomeMapProps {
  genomeId: number;
  width?: number;
  height?: number;
  onSnpClick?: (snp: MapSnp) => void;
}

interface ViewState {
  zoom: ZoomLevel;
  chromosome: string | null;
  viewStart: number;
  viewEnd: number;
}

interface LayerVisibility {
  health: boolean;
  pharma: boolean;
  traits: boolean;
  carrier: boolean;
}

// ---- Constants ----

const LAYER_COLORS: Record<string, string> = {
  health: '#A94442',
  pharma: '#2D5F8A',
  traits: '#4A7C59',
  carrier: '#C4953A',
};

const LAYER_LABELS: Record<string, string> = {
  health: 'Health Risks',
  pharma: 'Pharmacogenomics',
  traits: 'Traits',
  carrier: 'Carrier Status',
};

const MARKER_LAYER_MAP: Record<string, keyof LayerVisibility> = {
  health_risk: 'health',
  pharmacogenomics: 'pharma',
  trait: 'traits',
  carrier: 'carrier',
};

const LABEL_WIDTH = 48;
const TOP_MARGIN = 40;
const RIGHT_MARGIN = 80; // space for SNP count labels
const DEBOUNCE_MS = 300;
const REGION_THRESHOLD = 10_000_000;
const SNP_THRESHOLD = 100_000;

// ---- Utility ----

function formatPosition(pos: number): string {
  if (pos >= 1e6) return `${(pos / 1e6).toFixed(2)} Mb`;
  if (pos >= 1e3) return `${(pos / 1e3).toFixed(1)} Kb`;
  return `${pos} bp`;
}

function formatPositionShort(pos: number): string {
  if (pos >= 1e6) return `${(pos / 1e6).toFixed(1)}`;
  if (pos >= 1e3) return `${(pos / 1e3).toFixed(0)}K`;
  return String(pos);
}

function clamp(val: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, val));
}

// ---- Cache ----

interface RegionCacheEntry {
  key: string;
  data: MapSnp[];
  timestamp: number;
}

class RegionCache {
  private entries: Map<string, RegionCacheEntry> = new Map();
  private maxSize = 50;

  makeKey(chr: string, start: number, end: number): string {
    return `${chr}:${start}-${end}`;
  }

  get(chr: string, start: number, end: number): MapSnp[] | null {
    const key = this.makeKey(chr, start, end);
    const entry = this.entries.get(key);
    if (entry) {
      entry.timestamp = Date.now();
      return entry.data;
    }
    for (const e of this.entries.values()) {
      const [eChr, range] = e.key.split(':');
      if (eChr !== chr) continue;
      const [eStart, eEnd] = range.split('-').map(Number);
      if (eStart <= start && eEnd >= end) {
        return e.data.filter((s) => s.position >= start && s.position <= end);
      }
    }
    return null;
  }

  set(chr: string, start: number, end: number, data: MapSnp[]): void {
    if (this.entries.size >= this.maxSize) {
      let oldest: string | null = null;
      let oldestTime = Infinity;
      for (const [k, v] of this.entries) {
        if (v.timestamp < oldestTime) {
          oldest = k;
          oldestTime = v.timestamp;
        }
      }
      if (oldest) this.entries.delete(oldest);
    }
    const key = this.makeKey(chr, start, end);
    this.entries.set(key, { key, data, timestamp: Date.now() });
  }
}

// ---- Component ----

export default function GenomeMap({
  genomeId,
  width: propWidth,
  height: propHeight,
  onSnpClick,
}: GenomeMapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const regionCache = useRef(new RegionCache());
  const debounceTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rafId = useRef<number>(0);

  const [measuredWidth, setMeasuredWidth] = useState(960);
  const [measuredHeight, setMeasuredHeight] = useState(700);
  const width = propWidth ?? measuredWidth;
  const height = propHeight ?? measuredHeight;

  // Data
  const [layout, setLayout] = useState<GenomeLayout | null>(null);
  const [overlayMarkers, setOverlayMarkers] = useState<OverlayMarker[]>([]);
  const [chrDensity, setChrDensity] = useState<ChromosomeDensity | null>(null);
  const [regionSnps, setRegionSnps] = useState<MapSnp[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // View
  const [view, setView] = useState<ViewState>({
    zoom: 'genome',
    chromosome: null,
    viewStart: 0,
    viewEnd: 0,
  });

  const [layers, setLayers] = useState<LayerVisibility>({
    health: true,
    pharma: true,
    traits: true,
    carrier: true,
  });

  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    content: string;
  } | null>(null);

  const [hoveredSnp, setHoveredSnp] = useState<MapSnp | null>(null);
  const [hoveredSnpPos, setHoveredSnpPos] = useState<{ x: number; y: number } | null>(null);

  const [isDragging, setIsDragging] = useState(false);
  const dragStart = useRef<{ x: number; viewStart: number; viewEnd: number } | null>(null);

  // ---- Resize observer ----
  useEffect(() => {
    if (propWidth && propHeight) return;
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (!propWidth) setMeasuredWidth(entry.contentRect.width);
        if (!propHeight) setMeasuredHeight(Math.max(500, entry.contentRect.height));
      }
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [propWidth, propHeight]);

  // ---- Load data ----
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([getGenomeLayout(genomeId), getAnalysisOverlay(genomeId)])
      .then(([layoutData, markers]) => {
        if (cancelled) return;
        setLayout(layoutData);
        setOverlayMarkers(markers);
        setLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(String(err));
        setLoading(false);
      });
    return () => { cancelled = true; };
  }, [genomeId]);

  useEffect(() => {
    if (view.zoom !== 'chromosome' || !view.chromosome) return;
    let cancelled = false;
    getChromosomeDensity(genomeId, view.chromosome, 200)
      .then((density) => { if (!cancelled) setChrDensity(density); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [genomeId, view.zoom, view.chromosome]);

  useEffect(() => {
    if ((view.zoom !== 'region' && view.zoom !== 'snp') || !view.chromosome) return;
    const chr = view.chromosome;
    const start = Math.floor(view.viewStart);
    const end = Math.ceil(view.viewEnd);
    const cached = regionCache.current.get(chr, start, end);
    if (cached) { setRegionSnps(cached); return; }
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      getRegionSnps(genomeId, chr, start, end)
        .then((snps) => { regionCache.current.set(chr, start, end, snps); setRegionSnps(snps); })
        .catch(() => {});
    }, DEBOUNCE_MS);
    return () => { if (debounceTimer.current) clearTimeout(debounceTimer.current); };
  }, [genomeId, view.zoom, view.chromosome, view.viewStart, view.viewEnd]);

  // ---- Derived ----

  const maxChrLength = useMemo(() => {
    if (!layout) return 0;
    return Math.max(...layout.chromosomes.map((c) => c.maxPosition));
  }, [layout]);

  const chrMap = useMemo(() => {
    if (!layout) return new Map<string, ChromosomeLayout>();
    const map = new Map<string, ChromosomeLayout>();
    for (const c of layout.chromosomes) map.set(c.chromosome, c);
    return map;
  }, [layout]);

  const maxDensityCount = useMemo(() => {
    if (!layout) return 1;
    let max = 1;
    for (const chr of layout.chromosomes)
      for (const bin of chr.densityBins)
        if (bin.count > max) max = bin.count;
    return max;
  }, [layout]);

  // Stronger color scale: pumps the low end to avoid washed-out whites
  const densityColorScale = useMemo(() => {
    return (count: number) => {
      const t = Math.pow(count / maxDensityCount, 0.6); // gamma correction for visual punch
      return d3.interpolate('#E8EEF4', '#08306B')(t);
    };
  }, [maxDensityCount]);

  const chrDensityColorScale = useMemo(() => {
    if (!chrDensity) return densityColorScale;
    const max = Math.max(...chrDensity.bins.map((b) => b.count), 1);
    return (count: number) => {
      const t = Math.pow(count / max, 0.6);
      return d3.interpolate('#E8EEF4', '#08306B')(t);
    };
  }, [chrDensity, densityColorScale]);

  // Group overlay markers by chromosome for fast lookup
  const markersByChromosome = useMemo(() => {
    const map = new Map<string, OverlayMarker[]>();
    for (const m of overlayMarkers) {
      const arr = map.get(m.chromosome) || [];
      arr.push(m);
      map.set(m.chromosome, arr);
    }
    return map;
  }, [overlayMarkers]);

  // ---- Dynamic track sizing for genome view ----
  const genomeTrackMetrics = useMemo(() => {
    if (!layout) return { trackHeight: 20, trackGap: 4 };
    const chrCount = CHROMOSOMES.filter((c) => chrMap.has(c)).length;
    const availableHeight = height - TOP_MARGIN - 60; // 60px for bottom controls
    const totalGaps = (chrCount - 1) * 3;
    const trackHeight = Math.max(16, Math.min(28, Math.floor((availableHeight - totalGaps) / chrCount)));
    return { trackHeight, trackGap: 3 };
  }, [layout, height, chrMap]);

  // ---- Navigation ----

  const navigateToChromosome = useCallback((chr: string) => {
    const chrLayout = chrMap.get(chr);
    if (!chrLayout) return;
    setView({ zoom: 'chromosome', chromosome: chr, viewStart: chrLayout.minPosition, viewEnd: chrLayout.maxPosition });
    setChrDensity(null);
    setRegionSnps([]);
  }, [chrMap]);

  const navigateToGenome = useCallback(() => {
    setView({ zoom: 'genome', chromosome: null, viewStart: 0, viewEnd: 0 });
    setChrDensity(null);
    setRegionSnps([]);
    setHoveredSnp(null);
    setHoveredSnpPos(null);
  }, []);

  const zoomIn = useCallback(() => {
    if (view.zoom === 'genome') return;
    const span = view.viewEnd - view.viewStart;
    const center = (view.viewStart + view.viewEnd) / 2;
    const newSpan = span / 2;
    const chrLayout = view.chromosome ? chrMap.get(view.chromosome) : null;
    const minP = chrLayout?.minPosition ?? 0;
    const maxP = chrLayout?.maxPosition ?? 1;
    const newStart = clamp(center - newSpan / 2, minP, maxP);
    const newEnd = clamp(center + newSpan / 2, minP, maxP);
    const actualSpan = newEnd - newStart;
    let newZoom: ZoomLevel = actualSpan <= SNP_THRESHOLD ? 'snp' : actualSpan <= REGION_THRESHOLD ? 'region' : 'chromosome';
    setView({ zoom: newZoom, chromosome: view.chromosome, viewStart: newStart, viewEnd: newEnd });
  }, [view, chrMap]);

  const zoomOut = useCallback(() => {
    if (view.zoom === 'genome') return;
    if (view.zoom === 'chromosome') { navigateToGenome(); return; }
    const span = view.viewEnd - view.viewStart;
    const center = (view.viewStart + view.viewEnd) / 2;
    const newSpan = span * 2;
    const chrLayout = view.chromosome ? chrMap.get(view.chromosome) : null;
    const minP = chrLayout?.minPosition ?? 0;
    const maxP = chrLayout?.maxPosition ?? 1;
    let newStart = center - newSpan / 2;
    let newEnd = center + newSpan / 2;
    if (newStart <= minP && newEnd >= maxP) {
      setView({ zoom: 'chromosome', chromosome: view.chromosome, viewStart: minP, viewEnd: maxP });
      return;
    }
    newStart = clamp(newStart, minP, maxP);
    newEnd = clamp(newEnd, minP, maxP);
    const actualSpan = newEnd - newStart;
    const newZoom: ZoomLevel = actualSpan <= SNP_THRESHOLD ? 'snp' : actualSpan <= REGION_THRESHOLD ? 'region' : 'chromosome';
    setView({ zoom: newZoom, chromosome: view.chromosome, viewStart: newStart, viewEnd: newEnd });
  }, [view, chrMap, navigateToGenome]);

  const pan = useCallback((deltaFraction: number) => {
    if (view.zoom === 'genome') return;
    const span = view.viewEnd - view.viewStart;
    const delta = span * deltaFraction;
    const chrLayout = view.chromosome ? chrMap.get(view.chromosome) : null;
    const minP = chrLayout?.minPosition ?? 0;
    const maxP = chrLayout?.maxPosition ?? 1;
    const newStart = clamp(view.viewStart + delta, minP, maxP - span);
    const newEnd = newStart + span;
    setView((prev) => ({ ...prev, viewStart: newStart, viewEnd: Math.min(newEnd, maxP) }));
  }, [view, chrMap]);

  // ---- Rendering ----

  const renderGenomeView = useCallback((ctx: CanvasRenderingContext2D) => {
    if (!layout) return;
    const { trackHeight, trackGap } = genomeTrackMetrics;
    const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
    const xScale = d3.scaleLinear().domain([0, maxChrLength]).range([0, plotWidth]);
    const chrOrder = CHROMOSOMES.filter((c) => chrMap.has(c));

    // Render each chromosome track
    for (let i = 0; i < chrOrder.length; i++) {
      const chrName = chrOrder[i];
      const chrLayout = chrMap.get(chrName);
      if (!chrLayout) continue;

      const y = TOP_MARGIN + i * (trackHeight + trackGap);
      const trackWidth = xScale(chrLayout.maxPosition);

      // Background for full track extent
      ctx.fillStyle = '#F2F1EF';
      ctx.fillRect(LABEL_WIDTH, y, trackWidth, trackHeight);

      // Density heatmap bins
      for (const bin of chrLayout.densityBins) {
        const x = LABEL_WIDTH + xScale(bin.binStart);
        const w = Math.max(1, xScale(bin.binEnd - bin.binStart));
        ctx.fillStyle = densityColorScale(bin.count);
        ctx.fillRect(x, y, w, trackHeight);
      }

      // Thin border on track
      ctx.strokeStyle = '#D8D6D2';
      ctx.lineWidth = 0.5;
      ctx.strokeRect(LABEL_WIDTH, y, trackWidth, trackHeight);

      // Overlay markers as colored ticks WITHIN the track
      const chrMarkers = markersByChromosome.get(chrName) || [];
      for (const marker of chrMarkers) {
        const layerKey = MARKER_LAYER_MAP[marker.layer];
        if (layerKey && !layers[layerKey]) continue;
        const x = LABEL_WIDTH + xScale(marker.position);
        const layerColor = layerKey ? LAYER_COLORS[layerKey] : '#999';

        // Draw a colored tick that extends above and below the track
        ctx.strokeStyle = layerColor;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(x, y - 3);
        ctx.lineTo(x, y + trackHeight + 3);
        ctx.stroke();

        // Small triangle marker above track
        ctx.fillStyle = layerColor;
        ctx.beginPath();
        ctx.moveTo(x - 3, y - 3);
        ctx.lineTo(x + 3, y - 3);
        ctx.lineTo(x, y - 7);
        ctx.closePath();
        ctx.fill();
      }

      // SNP count label right of track
      ctx.fillStyle = colors.textMuted;
      ctx.font = `${Math.min(10, trackHeight - 4)}px "JetBrains Mono", monospace`;
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      ctx.fillText(
        chrLayout.snpCount.toLocaleString(),
        LABEL_WIDTH + trackWidth + 8,
        y + trackHeight / 2,
      );
    }

    ctx.textBaseline = 'alphabetic';

    // Summary stats at bottom
    const bottomY = TOP_MARGIN + chrOrder.length * (trackHeight + trackGap) + 16;
    ctx.fillStyle = colors.textMuted;
    ctx.font = '10px "Inter", sans-serif';
    ctx.textAlign = 'left';

    const totalSnps = layout.totalSnps.toLocaleString();
    const totalMarkers = overlayMarkers.length;
    const healthCount = overlayMarkers.filter((m) => m.layer === 'health_risk').length;
    const pharmaCount = overlayMarkers.filter((m) => m.layer === 'pharmacogenomics').length;
    const traitCount = overlayMarkers.filter((m) => m.layer === 'trait').length;
    const carrierCount = overlayMarkers.filter((m) => m.layer === 'carrier').length;

    ctx.fillText(`${totalSnps} SNPs across ${chrOrder.length} chromosomes`, LABEL_WIDTH, bottomY);

    if (totalMarkers > 0) {
      ctx.fillText(
        `${totalMarkers} annotated variants:`,
        LABEL_WIDTH,
        bottomY + 16,
      );

      let xOff = LABEL_WIDTH + ctx.measureText(`${totalMarkers} annotated variants:  `).width;
      const stats = [
        { count: healthCount, color: LAYER_COLORS.health, label: 'health' },
        { count: pharmaCount, color: LAYER_COLORS.pharma, label: 'pharma' },
        { count: traitCount, color: LAYER_COLORS.traits, label: 'traits' },
        { count: carrierCount, color: LAYER_COLORS.carrier, label: 'carrier' },
      ].filter((s) => s.count > 0);

      for (const stat of stats) {
        ctx.fillStyle = stat.color;
        ctx.fillRect(xOff, bottomY + 8, 8, 8);
        ctx.fillStyle = colors.textMuted;
        xOff += 12;
        const text = `${stat.count} ${stat.label}  `;
        ctx.fillText(text, xOff, bottomY + 16);
        xOff += ctx.measureText(text).width;
      }
    }

    // Scale bar in bottom right
    const scaleBarWidth = 100;
    const scaleBarX = LABEL_WIDTH + plotWidth - scaleBarWidth;
    const scaleBarY = bottomY - 4;
    const scaleBarValue = xScale.invert(scaleBarWidth);
    ctx.strokeStyle = colors.textMuted;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(scaleBarX, scaleBarY);
    ctx.lineTo(scaleBarX + scaleBarWidth, scaleBarY);
    ctx.moveTo(scaleBarX, scaleBarY - 3);
    ctx.lineTo(scaleBarX, scaleBarY + 3);
    ctx.moveTo(scaleBarX + scaleBarWidth, scaleBarY - 3);
    ctx.lineTo(scaleBarX + scaleBarWidth, scaleBarY + 3);
    ctx.stroke();
    ctx.fillStyle = colors.textMuted;
    ctx.font = '8px "JetBrains Mono", monospace';
    ctx.textAlign = 'center';
    ctx.fillText(formatPosition(scaleBarValue), scaleBarX + scaleBarWidth / 2, scaleBarY - 6);
  }, [layout, width, maxChrLength, chrMap, densityColorScale, overlayMarkers, layers, genomeTrackMetrics, markersByChromosome]);

  const renderChromosomeView = useCallback((ctx: CanvasRenderingContext2D) => {
    if (!view.chromosome) return;
    const chrLayout = chrMap.get(view.chromosome);
    if (!chrLayout) return;

    const bins = chrDensity ? chrDensity.bins : chrLayout.densityBins;
    const colorScale = chrDensity ? chrDensityColorScale : densityColorScale;

    const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
    const xScale = d3.scaleLinear().domain([view.viewStart, view.viewEnd]).range([0, plotWidth]);

    // === Track 1: Density heatmap (tall) ===
    const densityY = TOP_MARGIN + 30;
    const densityH = 60;

    ctx.fillStyle = '#F2F1EF';
    ctx.fillRect(LABEL_WIDTH, densityY, plotWidth, densityH);

    for (const bin of bins) {
      if (bin.binEnd < view.viewStart || bin.binStart > view.viewEnd) continue;
      const x = LABEL_WIDTH + xScale(Math.max(bin.binStart, view.viewStart));
      const xEnd = LABEL_WIDTH + xScale(Math.min(bin.binEnd, view.viewEnd));
      const w = Math.max(1, xEnd - x);
      ctx.fillStyle = colorScale(bin.count);
      ctx.fillRect(x, densityY, w, densityH);
    }

    ctx.strokeStyle = '#D8D6D2';
    ctx.lineWidth = 0.5;
    ctx.strokeRect(LABEL_WIDTH, densityY, plotWidth, densityH);

    // Label
    ctx.fillStyle = colors.textMuted;
    ctx.font = '8px "Inter", sans-serif';
    ctx.textAlign = 'right';
    ctx.fillText('density', LABEL_WIDTH - 6, densityY + densityH / 2 + 3);

    // === X-axis ===
    const axisY = densityY + densityH;
    const tickCount = Math.min(12, Math.max(4, Math.floor(plotWidth / 80)));
    const ticks = xScale.ticks(tickCount);

    ctx.strokeStyle = '#D8D6D2';
    ctx.fillStyle = colors.textMuted;
    ctx.font = '9px "JetBrains Mono", monospace';
    ctx.textAlign = 'center';
    ctx.lineWidth = 0.5;

    for (const tick of ticks) {
      const x = LABEL_WIDTH + xScale(tick);
      ctx.beginPath();
      ctx.moveTo(x, axisY);
      ctx.lineTo(x, axisY + 5);
      ctx.stroke();
      ctx.fillText(formatPositionShort(tick), x, axisY + 15);
    }

    ctx.fillStyle = colors.textMuted;
    ctx.font = '8px "Inter", sans-serif';
    ctx.textAlign = 'right';
    ctx.fillText('Mb', LABEL_WIDTH + plotWidth, axisY + 15);

    // === Track 2-5: Analysis overlay tracks ===
    const trackDefs = [
      { key: 'health', layer: 'health_risk', label: 'health risks', color: LAYER_COLORS.health },
      { key: 'pharma', layer: 'pharmacogenomics', label: 'pharma', color: LAYER_COLORS.pharma },
      { key: 'traits', layer: 'trait', label: 'traits', color: LAYER_COLORS.traits },
      { key: 'carrier', layer: 'carrier', label: 'carrier', color: LAYER_COLORS.carrier },
    ];

    const chrMarkers = markersByChromosome.get(view.chromosome) || [];
    const overlayTrackStart = axisY + 32;
    const overlayTrackH = 28;
    const overlayTrackGap = 4;

    let renderedTracks = 0;
    for (const trackDef of trackDefs) {
      const layerKey = trackDef.key as keyof LayerVisibility;
      if (!layers[layerKey]) continue;

      const trackMarkers = chrMarkers.filter((m) => m.layer === trackDef.layer);
      const trackY = overlayTrackStart + renderedTracks * (overlayTrackH + overlayTrackGap);

      // Track background
      ctx.fillStyle = trackDef.color + '08'; // very subtle tint
      ctx.fillRect(LABEL_WIDTH, trackY, plotWidth, overlayTrackH);

      // Track label
      ctx.fillStyle = trackDef.color;
      ctx.font = '8px "Inter", sans-serif';
      ctx.textAlign = 'right';
      ctx.fillText(trackDef.label, LABEL_WIDTH - 6, trackY + overlayTrackH / 2 + 3);

      // Baseline
      ctx.strokeStyle = trackDef.color + '30';
      ctx.lineWidth = 0.5;
      const baselineY = trackY + overlayTrackH / 2;
      ctx.beginPath();
      ctx.moveTo(LABEL_WIDTH, baselineY);
      ctx.lineTo(LABEL_WIDTH + plotWidth, baselineY);
      ctx.stroke();

      // Markers
      for (const marker of trackMarkers) {
        if (marker.position < view.viewStart || marker.position > view.viewEnd) continue;
        const x = LABEL_WIDTH + xScale(marker.position);

        // Vertical tick
        ctx.strokeStyle = trackDef.color;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(x, trackY + 2);
        ctx.lineTo(x, trackY + overlayTrackH - 2);
        ctx.stroke();

        // Dot at center
        ctx.beginPath();
        ctx.arc(x, baselineY, 3, 0, Math.PI * 2);
        ctx.fillStyle = trackDef.color;
        ctx.fill();

        // Label (gene or condition)
        const label = marker.label.length > 14 ? marker.label.slice(0, 13) + '\u2026' : marker.label;
        ctx.fillStyle = trackDef.color;
        ctx.font = '7px "JetBrains Mono", monospace';
        ctx.textAlign = 'center';
        ctx.fillText(label, x, trackY - 2);
      }

      // Count badge on right
      if (trackMarkers.length > 0) {
        ctx.fillStyle = trackDef.color;
        ctx.font = '9px "JetBrains Mono", monospace';
        ctx.textAlign = 'left';
        ctx.fillText(`${trackMarkers.length}`, LABEL_WIDTH + plotWidth + 8, trackY + overlayTrackH / 2 + 3);
      }

      renderedTracks++;
    }

    // === Bottom section: Notable findings list ===
    const findingsY = overlayTrackStart + renderedTracks * (overlayTrackH + overlayTrackGap) + 20;

    if (chrMarkers.length > 0) {
      ctx.fillStyle = colors.text;
      ctx.font = '10px "Inter", sans-serif';
      ctx.textAlign = 'left';
      ctx.fillText(`Notable variants on chromosome ${view.chromosome}`, LABEL_WIDTH, findingsY);

      // Table header
      const headerY = findingsY + 20;
      ctx.fillStyle = colors.textMuted;
      ctx.font = '8px "Inter", sans-serif';
      const cols = [LABEL_WIDTH, LABEL_WIDTH + 100, LABEL_WIDTH + 200, LABEL_WIDTH + 340];
      ctx.fillText('rsID', cols[0], headerY);
      ctx.fillText('Position', cols[1], headerY);
      ctx.fillText('Gene / Label', cols[2], headerY);
      ctx.fillText('Category', cols[3], headerY);

      // Thin rule
      ctx.strokeStyle = '#E8E6E3';
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(LABEL_WIDTH, headerY + 4);
      ctx.lineTo(LABEL_WIDTH + plotWidth, headerY + 4);
      ctx.stroke();

      // Rows (show up to what fits)
      const rowHeight = 16;
      const maxRows = Math.floor((height - headerY - 60) / rowHeight);
      const uniqueMarkers = new Map<string, OverlayMarker>();
      for (const m of chrMarkers) {
        if (!uniqueMarkers.has(m.rsid)) uniqueMarkers.set(m.rsid, m);
      }
      const sortedMarkers = [...uniqueMarkers.values()].sort((a, b) => a.position - b.position);
      const displayMarkers = sortedMarkers.slice(0, Math.max(0, maxRows));

      for (let i = 0; i < displayMarkers.length; i++) {
        const marker = displayMarkers[i];
        const rowY = headerY + 16 + i * rowHeight;
        const layerKey = MARKER_LAYER_MAP[marker.layer];
        const markerColor = layerKey ? LAYER_COLORS[layerKey] : colors.textMuted;

        // Color dot
        ctx.beginPath();
        ctx.arc(cols[0] - 8, rowY - 3, 2.5, 0, Math.PI * 2);
        ctx.fillStyle = markerColor;
        ctx.fill();

        ctx.font = '8px "JetBrains Mono", monospace';
        ctx.fillStyle = colors.accent;
        ctx.fillText(marker.rsid, cols[0], rowY);

        ctx.fillStyle = colors.textMuted;
        ctx.fillText(formatPosition(marker.position), cols[1], rowY);

        ctx.fillStyle = colors.text;
        ctx.font = '8px "Inter", sans-serif';
        const labelText = marker.label.length > 20 ? marker.label.slice(0, 19) + '\u2026' : marker.label;
        ctx.fillText(labelText, cols[2], rowY);

        ctx.fillStyle = markerColor;
        ctx.font = '8px "Inter", sans-serif';
        ctx.fillText(marker.significance || marker.layer, cols[3], rowY);
      }

      if (sortedMarkers.length > displayMarkers.length) {
        const moreY = headerY + 16 + displayMarkers.length * rowHeight;
        ctx.fillStyle = colors.textMuted;
        ctx.font = '8px "Inter", sans-serif';
        ctx.fillText(`+ ${sortedMarkers.length - displayMarkers.length} more variants`, cols[0], moreY);
      }
    } else {
      ctx.fillStyle = colors.textMuted;
      ctx.font = '10px "Inter", sans-serif';
      ctx.textAlign = 'left';
      ctx.fillText(
        `${chrLayout.snpCount.toLocaleString()} SNPs on this chromosome. No annotated variants found.`,
        LABEL_WIDTH,
        findingsY,
      );
      ctx.fillText(
        'Click anywhere on the density track to zoom in, or scroll to zoom.',
        LABEL_WIDTH,
        findingsY + 18,
      );
    }
  }, [view, chrMap, chrDensity, chrDensityColorScale, densityColorScale, width, height, overlayMarkers, layers, markersByChromosome]);

  const renderRegionView = useCallback((ctx: CanvasRenderingContext2D) => {
    if (!view.chromosome) return;
    const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
    const xScale = d3.scaleLinear().domain([view.viewStart, view.viewEnd]).range([0, plotWidth]);

    const trackY = TOP_MARGIN + 40;
    const trackHeight = 50;

    // Track background
    ctx.fillStyle = '#F5F5F3';
    ctx.fillRect(LABEL_WIDTH, trackY, plotWidth, trackHeight);
    ctx.strokeStyle = '#D8D6D2';
    ctx.lineWidth = 0.5;
    ctx.strokeRect(LABEL_WIDTH, trackY, plotWidth, trackHeight);

    // Track label
    ctx.fillStyle = colors.textMuted;
    ctx.font = '8px "Inter", sans-serif';
    ctx.textAlign = 'right';
    ctx.fillText('SNPs', LABEL_WIDTH - 6, trackY + trackHeight / 2 + 3);

    // SNP ticks
    for (const snp of regionSnps) {
      if (snp.position < view.viewStart || snp.position > view.viewEnd) continue;
      const x = LABEL_WIDTH + xScale(snp.position);
      const isHighlighted = snp.hasHealthRisk || snp.hasPharma || snp.hasTrait || snp.hasCarrier;
      const isAnnotated = snp.gene !== null;

      if (isHighlighted) {
        let tickColor: string = colors.accent;
        if (snp.hasHealthRisk && layers.health) tickColor = LAYER_COLORS.health;
        else if (snp.hasPharma && layers.pharma) tickColor = LAYER_COLORS.pharma;
        else if (snp.hasTrait && layers.traits) tickColor = LAYER_COLORS.traits;
        else if (snp.hasCarrier && layers.carrier) tickColor = LAYER_COLORS.carrier;

        ctx.strokeStyle = tickColor;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.moveTo(x, trackY - 8);
        ctx.lineTo(x, trackY + trackHeight + 8);
        ctx.stroke();

        // Triangle marker
        ctx.fillStyle = tickColor;
        ctx.beginPath();
        ctx.moveTo(x - 4, trackY - 8);
        ctx.lineTo(x + 4, trackY - 8);
        ctx.lineTo(x, trackY - 14);
        ctx.closePath();
        ctx.fill();
      } else if (isAnnotated) {
        ctx.strokeStyle = colors.accent;
        ctx.lineWidth = 1;
        ctx.globalAlpha = 0.6;
        ctx.beginPath();
        ctx.moveTo(x, trackY - 2);
        ctx.lineTo(x, trackY + trackHeight + 2);
        ctx.stroke();
        ctx.globalAlpha = 1;
      } else {
        ctx.strokeStyle = colors.textMuted;
        ctx.lineWidth = 0.5;
        ctx.globalAlpha = 0.25;
        ctx.beginPath();
        ctx.moveTo(x, trackY + 4);
        ctx.lineTo(x, trackY + trackHeight - 4);
        ctx.stroke();
        ctx.globalAlpha = 1;
      }

      // Gene labels for annotated
      if (isAnnotated && snp.gene) {
        ctx.fillStyle = colors.text;
        ctx.font = '7px "JetBrains Mono", monospace';
        ctx.textAlign = 'center';
        ctx.fillText(snp.gene, x, trackY + trackHeight + 20);
      }
    }

    // Overlay badges above track
    for (const marker of overlayMarkers) {
      if (marker.chromosome !== view.chromosome) continue;
      const layerKey = MARKER_LAYER_MAP[marker.layer];
      if (layerKey && !layers[layerKey]) continue;
      if (marker.position < view.viewStart || marker.position > view.viewEnd) continue;
      const x = LABEL_WIDTH + xScale(marker.position);
      const layerColor = layerKey ? LAYER_COLORS[layerKey] : '#999';
      const badgeText = marker.label.length > 16 ? marker.label.slice(0, 15) + '\u2026' : marker.label;
      ctx.font = '8px "JetBrains Mono", monospace';
      const textWidth = ctx.measureText(badgeText).width;
      const badgeW = textWidth + 8;
      const badgeH = 14;
      const badgeX = x - badgeW / 2;
      const badgeY = trackY - 32;

      ctx.fillStyle = layerColor + '20';
      ctx.fillRect(badgeX, badgeY, badgeW, badgeH);
      ctx.strokeStyle = layerColor;
      ctx.lineWidth = 0.5;
      ctx.strokeRect(badgeX, badgeY, badgeW, badgeH);
      ctx.fillStyle = layerColor;
      ctx.textAlign = 'center';
      ctx.fillText(badgeText, x, badgeY + 10);
    }

    // X axis
    const tickCount = Math.min(10, Math.max(4, Math.floor(plotWidth / 80)));
    const ticks = xScale.ticks(tickCount);
    ctx.strokeStyle = '#D8D6D2';
    ctx.fillStyle = colors.textMuted;
    ctx.font = '9px "JetBrains Mono", monospace';
    ctx.textAlign = 'center';
    ctx.lineWidth = 0.5;

    for (const tick of ticks) {
      const x = LABEL_WIDTH + xScale(tick);
      ctx.beginPath();
      ctx.moveTo(x, trackY + trackHeight + 30);
      ctx.lineTo(x, trackY + trackHeight + 35);
      ctx.stroke();
      ctx.fillText(formatPositionShort(tick), x, trackY + trackHeight + 46);
    }

    // Summary below
    const summaryY = trackY + trackHeight + 64;
    const highlighted = regionSnps.filter((s) => s.hasHealthRisk || s.hasPharma || s.hasTrait || s.hasCarrier);
    const annotated = regionSnps.filter((s) => s.gene !== null);
    ctx.fillStyle = colors.textMuted;
    ctx.font = '10px "Inter", sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText(
      `${regionSnps.length} SNPs in view · ${annotated.length} with gene annotations · ${highlighted.length} with analysis flags`,
      LABEL_WIDTH,
      summaryY,
    );
    ctx.fillText(
      'Drag to pan · Scroll to zoom · Click highlighted variants for details',
      LABEL_WIDTH,
      summaryY + 16,
    );
  }, [view, width, regionSnps, overlayMarkers, layers]);

  const renderSnpView = useCallback((ctx: CanvasRenderingContext2D) => {
    if (!view.chromosome) return;
    const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
    const xScale = d3.scaleLinear().domain([view.viewStart, view.viewEnd]).range([0, plotWidth]);

    const trackY = TOP_MARGIN + 60;
    const trackHeight = 40;

    ctx.fillStyle = '#FAFAF8';
    ctx.fillRect(LABEL_WIDTH, trackY, plotWidth, trackHeight);
    ctx.strokeStyle = '#D8D6D2';
    ctx.lineWidth = 0.5;
    ctx.strokeRect(LABEL_WIDTH, trackY, plotWidth, trackHeight);

    const visibleSnps = regionSnps.filter(
      (s) => s.position >= view.viewStart && s.position <= view.viewEnd,
    );

    for (const snp of visibleSnps) {
      const x = LABEL_WIDTH + xScale(snp.position);
      const isHighlighted = snp.hasHealthRisk || snp.hasPharma || snp.hasTrait || snp.hasCarrier;
      ctx.strokeStyle = isHighlighted ? LAYER_COLORS.health : colors.accent;
      ctx.lineWidth = isHighlighted ? 2 : 1;
      ctx.beginPath();
      ctx.moveTo(x, trackY - 4);
      ctx.lineTo(x, trackY + trackHeight + 4);
      ctx.stroke();

      // rsID label rotated
      ctx.fillStyle = colors.text;
      ctx.font = '8px "JetBrains Mono", monospace';
      ctx.textAlign = 'center';
      ctx.save();
      ctx.translate(x, trackY - 8);
      ctx.rotate(-Math.PI / 4);
      ctx.fillText(snp.rsid, 0, 0);
      ctx.restore();

      // Genotype below
      ctx.fillStyle = colors.accent;
      ctx.font = 'bold 9px "JetBrains Mono", monospace';
      ctx.textAlign = 'center';
      const genotypeLabel = snp.genotype.length === 2 ? `${snp.genotype[0]}/${snp.genotype[1]}` : snp.genotype;
      ctx.fillText(genotypeLabel, x, trackY + trackHeight + 16);

      if (snp.gene) {
        ctx.fillStyle = colors.textMuted;
        ctx.font = '7px "Inter", sans-serif';
        ctx.fillText(snp.gene, x, trackY + trackHeight + 26);
      }
    }

    // X axis
    const ticks = xScale.ticks(8);
    ctx.strokeStyle = '#D8D6D2';
    ctx.fillStyle = colors.textMuted;
    ctx.font = '8px "JetBrains Mono", monospace';
    ctx.textAlign = 'center';
    ctx.lineWidth = 0.5;
    for (const tick of ticks) {
      const x = LABEL_WIDTH + xScale(tick);
      ctx.beginPath();
      ctx.moveTo(x, trackY + trackHeight + 34);
      ctx.lineTo(x, trackY + trackHeight + 38);
      ctx.stroke();
      ctx.fillText(formatPosition(tick), x, trackY + trackHeight + 48);
    }
  }, [view, width, regionSnps]);

  // ---- Main render loop ----
  const renderCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    switch (view.zoom) {
      case 'genome': renderGenomeView(ctx); break;
      case 'chromosome': renderChromosomeView(ctx); break;
      case 'region': renderRegionView(ctx); break;
      case 'snp': renderSnpView(ctx); break;
    }
  }, [width, height, view.zoom, renderGenomeView, renderChromosomeView, renderRegionView, renderSnpView]);

  useEffect(() => {
    cancelAnimationFrame(rafId.current);
    rafId.current = requestAnimationFrame(renderCanvas);
    return () => cancelAnimationFrame(rafId.current);
  }, [renderCanvas]);

  // ---- SVG labels for genome view ----
  const svgGenomeLabels = useMemo(() => {
    if (view.zoom !== 'genome' || !layout) return null;
    const { trackHeight, trackGap } = genomeTrackMetrics;
    const chrOrder = CHROMOSOMES.filter((c) => chrMap.has(c));
    return chrOrder.map((chrName, i) => {
      const y = TOP_MARGIN + i * (trackHeight + trackGap);
      return (
        <text
          key={chrName}
          x={LABEL_WIDTH - 6}
          y={y + trackHeight / 2 + 3.5}
          textAnchor="end"
          fill={colors.text}
          fontSize={Math.min(11, trackHeight - 4)}
          fontFamily="'JetBrains Mono', monospace"
          fontWeight={600}
          style={{ cursor: 'pointer' }}
          onClick={() => navigateToChromosome(chrName)}
        >
          {chrName}
        </text>
      );
    });
  }, [view.zoom, layout, chrMap, navigateToChromosome, genomeTrackMetrics]);

  // ---- Event handlers ----

  const handleWheel = useCallback((e: ReactWheelEvent) => {
    e.preventDefault();
    if (view.zoom === 'genome') {
      if (e.ctrlKey || Math.abs(e.deltaY) > 50) {
        const rect = containerRef.current?.getBoundingClientRect();
        if (!rect) return;
        const mouseY = e.clientY - rect.top;
        const { trackHeight, trackGap } = genomeTrackMetrics;
        const chrOrder = CHROMOSOMES.filter((c) => chrMap.has(c));
        const chrIdx = Math.floor((mouseY - TOP_MARGIN) / (trackHeight + trackGap));
        if (chrIdx >= 0 && chrIdx < chrOrder.length && e.deltaY < 0) {
          navigateToChromosome(chrOrder[chrIdx]);
        }
      }
      return;
    }
    if (e.deltaY < 0) zoomIn();
    else zoomOut();
  }, [view.zoom, chrMap, navigateToChromosome, zoomIn, zoomOut, genomeTrackMetrics]);

  const handleMouseDown = useCallback((e: ReactMouseEvent) => {
    if (view.zoom === 'genome') return;
    setIsDragging(true);
    dragStart.current = { x: e.clientX, viewStart: view.viewStart, viewEnd: view.viewEnd };
  }, [view]);

  const handleMouseMove = useCallback((e: ReactMouseEvent) => {
    if (view.zoom === 'genome' && layout) {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const mouseY = e.clientY - rect.top;
      const mouseX = e.clientX - rect.left;
      const { trackHeight, trackGap } = genomeTrackMetrics;
      const chrOrder = CHROMOSOMES.filter((c) => chrMap.has(c));
      const chrIdx = Math.floor((mouseY - TOP_MARGIN) / (trackHeight + trackGap));

      if (chrIdx >= 0 && chrIdx < chrOrder.length) {
        const chrName = chrOrder[chrIdx];
        const chrLayout = chrMap.get(chrName);
        if (chrLayout) {
          const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
          const xScale = d3.scaleLinear().domain([0, maxChrLength]).range([0, plotWidth]);
          const pos = xScale.invert(mouseX - LABEL_WIDTH);
          if (pos >= 0 && pos <= chrLayout.maxPosition) {
            setTooltip({
              x: mouseX + 12, y: mouseY - 8,
              content: `Chr ${chrName} \u00b7 ${formatPosition(pos)} \u00b7 ${chrLayout.snpCount.toLocaleString()} SNPs`,
            });
          } else {
            setTooltip(null);
          }
        }
      } else {
        setTooltip(null);
      }
    }

    if (view.zoom === 'snp' && regionSnps.length > 0) {
      const rect = containerRef.current?.getBoundingClientRect();
      if (!rect) return;
      const mouseX = e.clientX - rect.left;
      const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
      const xScale = d3.scaleLinear().domain([view.viewStart, view.viewEnd]).range([0, plotWidth]);
      const pos = xScale.invert(mouseX - LABEL_WIDTH);
      let nearest: MapSnp | null = null;
      let nearestDist = Infinity;
      for (const snp of regionSnps) {
        const dist = Math.abs(snp.position - pos);
        if (dist < nearestDist) { nearestDist = dist; nearest = snp; }
      }
      const span = view.viewEnd - view.viewStart;
      if (nearest && nearestDist < span * 0.02) {
        setHoveredSnp(nearest);
        setHoveredSnpPos({ x: LABEL_WIDTH + xScale(nearest.position), y: TOP_MARGIN + 20 });
      } else {
        setHoveredSnp(null);
        setHoveredSnpPos(null);
      }
    }

    if (isDragging && dragStart.current) {
      const dx = e.clientX - dragStart.current.x;
      const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
      const span = dragStart.current.viewEnd - dragStart.current.viewStart;
      const positionDelta = -(dx / plotWidth) * span;
      const chrLayout = view.chromosome ? chrMap.get(view.chromosome) : null;
      const minP = chrLayout?.minPosition ?? 0;
      const maxP = chrLayout?.maxPosition ?? 1;
      const newStart = clamp(dragStart.current.viewStart + positionDelta, minP, maxP - span);
      const newEnd = newStart + span;
      setView((prev) => ({ ...prev, viewStart: newStart, viewEnd: Math.min(newEnd, maxP) }));
    }
  }, [view, layout, isDragging, width, maxChrLength, chrMap, regionSnps, genomeTrackMetrics]);

  const handleMouseUp = useCallback(() => { setIsDragging(false); dragStart.current = null; }, []);

  const handleMouseLeave = useCallback(() => {
    setTooltip(null);
    setIsDragging(false);
    dragStart.current = null;
    if (view.zoom !== 'snp') { setHoveredSnp(null); setHoveredSnpPos(null); }
  }, [view.zoom]);

  const handleClick = useCallback((e: ReactMouseEvent) => {
    if (isDragging) return;
    const rect = containerRef.current?.getBoundingClientRect();
    if (!rect) return;
    const mouseX = e.clientX - rect.left;
    const mouseY = e.clientY - rect.top;

    if (view.zoom === 'genome' && layout) {
      const { trackHeight, trackGap } = genomeTrackMetrics;
      const chrOrder = CHROMOSOMES.filter((c) => chrMap.has(c));
      const chrIdx = Math.floor((mouseY - TOP_MARGIN) / (trackHeight + trackGap));
      if (chrIdx >= 0 && chrIdx < chrOrder.length) navigateToChromosome(chrOrder[chrIdx]);
      return;
    }

    if (view.zoom === 'chromosome') {
      const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
      const xScale = d3.scaleLinear().domain([view.viewStart, view.viewEnd]).range([0, plotWidth]);
      const clickPos = xScale.invert(mouseX - LABEL_WIDTH);
      const regionSpan = REGION_THRESHOLD;
      const chrLayout = view.chromosome ? chrMap.get(view.chromosome) : null;
      const minP = chrLayout?.minPosition ?? 0;
      const maxP = chrLayout?.maxPosition ?? 1;
      const newStart = clamp(clickPos - regionSpan / 2, minP, maxP - regionSpan);
      const newEnd = Math.min(newStart + regionSpan, maxP);
      setView({ zoom: 'region', chromosome: view.chromosome, viewStart: newStart, viewEnd: newEnd });
      return;
    }

    if ((view.zoom === 'region' || view.zoom === 'snp') && onSnpClick) {
      const plotWidth = width - LABEL_WIDTH - RIGHT_MARGIN;
      const xScale = d3.scaleLinear().domain([view.viewStart, view.viewEnd]).range([0, plotWidth]);
      const clickPos = xScale.invert(mouseX - LABEL_WIDTH);
      const span = view.viewEnd - view.viewStart;
      let nearest: MapSnp | null = null;
      let nearestDist = Infinity;
      for (const snp of regionSnps) {
        const dist = Math.abs(snp.position - clickPos);
        if (dist < nearestDist) { nearestDist = dist; nearest = snp; }
      }
      if (nearest && nearestDist < span * 0.02) onSnpClick(nearest);
    }
  }, [view, layout, width, chrMap, navigateToChromosome, isDragging, regionSnps, onSnpClick, genomeTrackMetrics]);

  const handleKeyDown = useCallback((e: ReactKeyboardEvent) => {
    switch (e.key) {
      case 'ArrowLeft': e.preventDefault(); pan(-0.1); break;
      case 'ArrowRight': e.preventDefault(); pan(0.1); break;
      case '+': case '=': e.preventDefault(); zoomIn(); break;
      case '-': e.preventDefault(); zoomOut(); break;
      case 'Escape':
        e.preventDefault();
        if (view.zoom === 'snp' || view.zoom === 'region') {
          if (view.chromosome) navigateToChromosome(view.chromosome);
        } else if (view.zoom === 'chromosome') navigateToGenome();
        break;
    }
  }, [pan, zoomIn, zoomOut, view, navigateToChromosome, navigateToGenome]);

  // ---- Breadcrumb ----
  const breadcrumb = useMemo(() => {
    const items: { label: string; onClick?: () => void }[] = [];
    items.push({ label: 'Genome', onClick: view.zoom !== 'genome' ? navigateToGenome : undefined });
    if (view.chromosome) {
      items.push({
        label: `Chr ${view.chromosome}`,
        onClick: view.zoom !== 'chromosome' ? () => navigateToChromosome(view.chromosome!) : undefined,
      });
    }
    if (view.zoom === 'region' || view.zoom === 'snp') {
      items.push({ label: `${formatPosition(view.viewStart)} \u2013 ${formatPosition(view.viewEnd)}` });
    }
    return items;
  }, [view, navigateToGenome, navigateToChromosome]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Loading genome map...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">Error: {error}</p>
      </div>
    );
  }

  if (!layout) {
    return (
      <div className="flex items-center justify-center h-[60vh]">
        <p className="text-sm text-text-muted">No data available.</p>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="relative select-none outline-none"
      style={{ width: propWidth ? `${propWidth}px` : '100%', height: propHeight ? `${propHeight}px` : '100%', minHeight: 500 }}
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onWheel={handleWheel}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseLeave}
      onClick={handleClick}
    >
      <canvas
        ref={canvasRef}
        className="absolute top-0 left-0"
        style={{ cursor: view.zoom === 'genome' ? 'pointer' : isDragging ? 'grabbing' : 'grab' }}
      />

      <svg
        ref={svgRef}
        width={width}
        height={height}
        className="absolute top-0 left-0 pointer-events-none"
        style={{ fontFamily: "'JetBrains Mono', monospace" }}
      >
        <g style={{ pointerEvents: 'all' }}>{svgGenomeLabels}</g>

        {view.zoom !== 'genome' && view.chromosome && (
          <text
            x={LABEL_WIDTH}
            y={TOP_MARGIN + 20}
            fill={colors.text}
            fontSize={13}
            fontFamily="'JetBrains Mono', monospace"
            fontWeight={600}
          >
            Chromosome {view.chromosome}
            <tspan fill={colors.textMuted} fontWeight={400} fontSize={10} dx={12}>
              {view.zoom === 'chromosome' && chrMap.get(view.chromosome)
                ? `${chrMap.get(view.chromosome)!.snpCount.toLocaleString()} SNPs`
                : `${regionSnps.length.toLocaleString()} SNPs in view`}
            </tspan>
          </text>
        )}
      </svg>

      {/* Breadcrumb */}
      <div className="absolute top-2 left-2 flex items-center gap-1 text-xs" style={{ pointerEvents: 'all' }}>
        {breadcrumb.map((item, i) => (
          <span key={i} className="flex items-center gap-1">
            {i > 0 && <span className="text-text-muted">&rsaquo;</span>}
            {item.onClick ? (
              <button onClick={(e) => { e.stopPropagation(); item.onClick!(); }} className="text-accent hover:underline font-mono">
                {item.label}
              </button>
            ) : (
              <span className="text-text font-mono font-semibold">{item.label}</span>
            )}
          </span>
        ))}
      </div>

      {/* Legend (top right) */}
      <div className="absolute top-2 flex items-center gap-3 text-xs" style={{ right: 16, pointerEvents: 'none' }}>
        {Object.entries(LAYER_COLORS).map(([key, color]) => {
          const layerKey = key as keyof LayerVisibility;
          if (!layers[layerKey]) return null;
          return (
            <span key={key} className="flex items-center gap-1">
              <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: color }} />
              <span className="text-text-muted" style={{ fontFamily: "'Inter', sans-serif" }}>{LAYER_LABELS[key]}</span>
            </span>
          );
        })}
      </div>

      {/* Layer toggle panel */}
      <div className="absolute bottom-4 bg-surface border border-border rounded-sm p-3 shadow-sm" style={{ right: 16, pointerEvents: 'all', zIndex: 10 }}>
        <p className="text-[9px] font-semibold uppercase tracking-wider text-text-muted mb-2" style={{ fontFamily: "'Inter', sans-serif" }}>Layers</p>
        {Object.entries(LAYER_LABELS).map(([key, label]) => {
          const layerKey = key as keyof LayerVisibility;
          return (
            <label key={key} className="flex items-center gap-2 text-xs text-text cursor-pointer mb-1 last:mb-0" style={{ fontFamily: "'Inter', sans-serif" }}>
              <input type="checkbox" checked={layers[layerKey]} onChange={() => setLayers((prev) => ({ ...prev, [layerKey]: !prev[layerKey] }))} className="accent-accent" />
              <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: LAYER_COLORS[key] }} />
              {label}
            </label>
          );
        })}
      </div>

      {/* Tooltip */}
      {tooltip && (
        <div className="absolute bg-surface border border-border rounded-sm px-2 py-1 text-xs font-mono text-text shadow-sm pointer-events-none" style={{ left: tooltip.x, top: tooltip.y, zIndex: 20 }}>
          {tooltip.content}
        </div>
      )}

      {/* SNP hover card */}
      {hoveredSnp && hoveredSnpPos && (
        <div className="absolute bg-surface border border-border rounded-sm p-3 shadow-sm pointer-events-none" style={{ left: hoveredSnpPos.x + 8, top: hoveredSnpPos.y, zIndex: 20, minWidth: 180 }}>
          <p className="text-xs font-mono font-semibold text-accent mb-1">{hoveredSnp.rsid}</p>
          <p className="text-xs text-text-muted font-mono">{hoveredSnp.chromosome}:{hoveredSnp.position.toLocaleString()}</p>
          <p className="text-xs font-mono text-text mt-1">
            Genotype: <span className="font-semibold">{hoveredSnp.genotype.length === 2 ? `${hoveredSnp.genotype[0]}/${hoveredSnp.genotype[1]}` : hoveredSnp.genotype}</span>
          </p>
          {hoveredSnp.gene && <p className="text-xs text-text-muted mt-1">Gene: <span className="text-text">{hoveredSnp.gene}</span></p>}
          {hoveredSnp.clinicalSignificance && <p className="text-xs text-text-muted mt-1">Clinical: <span className="text-text">{hoveredSnp.clinicalSignificance}</span></p>}
          {hoveredSnp.condition && <p className="text-xs text-text-muted mt-1">Condition: <span className="text-text">{hoveredSnp.condition}</span></p>}
          <div className="flex gap-2 mt-2">
            {hoveredSnp.hasHealthRisk && <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: LAYER_COLORS.health }} />}
            {hoveredSnp.hasPharma && <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: LAYER_COLORS.pharma }} />}
            {hoveredSnp.hasTrait && <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: LAYER_COLORS.traits }} />}
            {hoveredSnp.hasCarrier && <span className="inline-block w-2 h-2 rounded-full" style={{ backgroundColor: LAYER_COLORS.carrier }} />}
          </div>
        </div>
      )}

      {/* Zoom controls */}
      <div className="absolute bottom-4 left-4 flex items-center gap-2" style={{ pointerEvents: 'all', zIndex: 10 }}>
        <button
          onClick={(e) => { e.stopPropagation(); zoomOut(); }}
          disabled={view.zoom === 'genome'}
          aria-label="Zoom out"
          className="w-6 h-6 flex items-center justify-center text-xs border border-border rounded-sm bg-surface text-text disabled:opacity-30 hover:bg-border transition-colors"
        >
          &minus;
        </button>
        <span className="text-xs text-text-muted font-mono px-1" style={{ minWidth: 80, textAlign: 'center' }}>
          {view.zoom === 'genome' ? 'Genome' : view.zoom === 'chromosome' ? 'Chromosome' : view.zoom === 'region' ? 'Region' : 'SNP'}
        </span>
        <button
          onClick={(e) => { e.stopPropagation(); zoomIn(); }}
          disabled={view.zoom === 'genome'}
          aria-label="Zoom in"
          className="w-6 h-6 flex items-center justify-center text-xs border border-border rounded-sm bg-surface text-text disabled:opacity-30 hover:bg-border transition-colors"
        >
          +
        </button>
      </div>
    </div>
  );
}
