import React, { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import * as d3 from 'd3';
import type { GenomeLayout } from '../lib/tauri-bridge';
import { CHROMOSOMES } from '../lib/constants';

// ---- Types ----

export interface KaryogramFinding {
  id: string;
  category: 'health' | 'pharma' | 'traits' | 'carrier';
  title: string;
  subtitle: string;
  detail: string;
  chromosome: string | null;
  position: number | null;
  rsid: string | null;
  significance: string;
  color: string;
}

export interface CompactKaryogramProps {
  layout: GenomeLayout;
  findings: KaryogramFinding[];
  highlighted: KaryogramFinding | null;
  onFindingClick: (f: KaryogramFinding) => void;
  relevantIds?: Set<string>;
  activeQueryLabel?: string;
  className?: string;
}

// ---- Component ----

export default function CompactKaryogram({
  layout, findings, highlighted, onFindingClick, relevantIds, className,
}: CompactKaryogramProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [containerWidth, setContainerWidth] = useState(800);
  const containerRef = useRef<HTMLDivElement>(null);
  const [hoveredFinding, setHoveredFinding] = useState<KaryogramFinding | null>(null);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setContainerWidth(entry.contentRect.width);
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const chrMap = useMemo(() => {
    const map = new Map<string, { minPosition: number; maxPosition: number; snpCount: number }>();
    for (const c of layout.chromosomes) map.set(c.chromosome, c);
    return map;
  }, [layout]);

  const allChrs = CHROMOSOMES.filter((c) => chrMap.has(c));
  const midpoint = Math.ceil(allChrs.length / 2);
  const row1 = allChrs.slice(0, midpoint);
  const row2 = allChrs.slice(midpoint);

  const rowPadding = 8;
  const chrGap = 4;
  const chrHeight = 18;
  const markerZone = 24;
  const labelHeight = 14;
  const totalRowHeight = markerZone + chrHeight + labelHeight;
  const svgHeight = totalRowHeight * 2 + rowPadding + 12;
  const hasDimming = relevantIds != null && relevantIds.size > 0;

  const getChrX = useCallback(
    (chr: string, pos: number, rowChrs: string[],  rowY: number) => {
      const totalGaps = (rowChrs.length - 1) * chrGap;
      const availableWidth = containerWidth - 12;
      const totalLength = rowChrs.reduce((sum, c) => sum + (chrMap.get(c)?.maxPosition ?? 0), 0);
      let xOffset = 6;
      for (const c of rowChrs) {
        const chrData = chrMap.get(c);
        if (!chrData) continue;
        const chrWidth = ((chrData.maxPosition / totalLength) * (availableWidth - totalGaps));
        if (c === chr) {
          const fraction = pos / chrData.maxPosition;
          return { x: xOffset + fraction * chrWidth, y: rowY, chrWidth };
        }
        xOffset += chrWidth + chrGap;
      }
      return null;
    }, [containerWidth, chrMap],
  );

  const renderRow = useCallback(
    (rowChrs: string[], rowY: number) => {
      const totalGaps = (rowChrs.length - 1) * chrGap;
      const availableWidth = containerWidth - 12;
      const totalLength = rowChrs.reduce((sum, c) => sum + (chrMap.get(c)?.maxPosition ?? 0), 0);
      const elements: React.JSX.Element[] = [];
      let xOffset = 6;
      for (const chr of rowChrs) {
        const chrData = chrMap.get(chr);
        if (!chrData) continue;
        const chrWidth = ((chrData.maxPosition / totalLength) * (availableWidth - totalGaps));
        elements.push(<rect key={`chr-${chr}`} x={xOffset} y={rowY + markerZone} width={chrWidth} height={chrHeight} rx={chrHeight / 2} fill="#E0DDD8" stroke="#D0CDC8" strokeWidth={0.5} />);
        const densityBins = layout.chromosomes.find((c) => c.chromosome === chr)?.densityBins || [];
        const maxBinCount = Math.max(...densityBins.map((b) => b.count), 1);
        for (const bin of densityBins) {
          const bx = xOffset + (bin.binStart / chrData.maxPosition) * chrWidth;
          const bw = Math.max(0.5, ((bin.binEnd - bin.binStart) / chrData.maxPosition) * chrWidth);
          const intensity = Math.pow(bin.count / maxBinCount, 0.5);
          const color = d3.interpolate('#E0DDD8', '#6B8CAE')(intensity);
          elements.push(<rect key={`density-${chr}-${bin.binStart}`} x={bx} y={rowY + markerZone} width={bw} height={chrHeight} fill={color} rx={bx === xOffset ? chrHeight / 2 : 0} />);
        }
        elements.push(<text key={`label-${chr}`} x={xOffset + chrWidth / 2} y={rowY + markerZone + chrHeight + 11} textAnchor="middle" fill="#6B6965" fontSize={8} fontFamily="'JetBrains Mono', monospace">{chr}</text>);
        xOffset += chrWidth + chrGap;
      }
      return elements;
    }, [containerWidth, chrMap, layout],
  );

  const renderMarkers = useCallback(() => {
    const elements: React.JSX.Element[] = [];
    for (const finding of findings) {
      if (!finding.chromosome || finding.position === null) continue;
      const row1Set = new Set(row1);
      const isRow1 = row1Set.has(finding.chromosome as typeof CHROMOSOMES[number]);
      const rowChrs = isRow1 ? row1 : row2;
      const rowY = isRow1 ? 0 : totalRowHeight + rowPadding;
      const pos = getChrX(finding.chromosome, finding.position, rowChrs, rowY);
      if (!pos) continue;

      const isHl = highlighted?.id === finding.id;
      const isHovered = hoveredFinding?.id === finding.id;
      const markerY = pos.y + markerZone - 4;
      const opacity = isHl || isHovered ? 1 : hasDimming ? (relevantIds!.has(finding.id) ? 0.9 : 0.12) : 0.7;

      // Line
      elements.push(<line key={`line-${finding.id}`} x1={pos.x} x2={pos.x} y1={markerY} y2={pos.y + markerZone + chrHeight + 2} stroke={finding.color} strokeWidth={isHl || isHovered ? 2 : 1} opacity={opacity} />);

      // Triangle — with hover handlers
      elements.push(
        <polygon
          key={`marker-${finding.id}`}
          points={`${pos.x - 4},${markerY - 1} ${pos.x + 4},${markerY - 1} ${pos.x},${markerY - 7}`}
          fill={finding.color}
          opacity={isHl || isHovered ? 1 : Math.min(opacity + 0.1, 1)}
          style={{ cursor: 'pointer' }}
          onClick={(e) => { e.stopPropagation(); onFindingClick(finding); }}
          onMouseEnter={() => { setHoveredFinding(finding); setHoverPos({ x: pos.x, y: markerY - 10 }); }}
          onMouseLeave={() => { setHoveredFinding(null); setHoverPos(null); }}
        />,
      );

      // Label for highlighted or hovered
      if (isHl || isHovered) {
        const labelText = finding.title.length > 24 ? finding.title.slice(0, 23) + '\u2026' : finding.title;
        elements.push(
          <text key={`label-${finding.id}`} x={pos.x} y={markerY - 10} textAnchor="middle" fill={finding.color} fontSize={8} fontWeight={600} fontFamily="'JetBrains Mono', monospace">
            {labelText}
          </text>,
        );
      }
    }
    return elements;
  }, [findings, highlighted, hoveredFinding, containerWidth, getChrX, row1, row2, onFindingClick, totalRowHeight, hasDimming, relevantIds]);

  return (
    <div ref={containerRef} className={`relative ${className || ''}`}>
      <div className="flex items-baseline justify-between mb-1">
        <p className="text-[9px] uppercase tracking-wider text-text-muted font-semibold">
          Chromosomes 1–22, X, Y
        </p>
        <p className="text-[9px] text-text-muted">
          {findings.length} findings · Hover for details
        </p>
      </div>
      <svg ref={svgRef} width={containerWidth} height={svgHeight} className="block">
        {renderRow(row1, 0)}
        {renderRow(row2, totalRowHeight + rowPadding)}
        {renderMarkers()}
      </svg>

      {/* Hover tooltip */}
      {hoveredFinding && hoverPos && (
        <div
          className="absolute bg-surface/85 backdrop-blur-sm border border-border rounded-sm px-2.5 py-1.5 shadow-sm pointer-events-none z-10"
          style={{ left: Math.min(hoverPos.x - 80, containerWidth - 180), top: Math.max(0, hoverPos.y - 50), maxWidth: 200 }}
        >
          <p className="text-[10px] font-mono font-semibold text-accent">{hoveredFinding.rsid || hoveredFinding.title}</p>
          <p className="text-[10px] text-text font-medium">{hoveredFinding.title}</p>
          {hoveredFinding.subtitle && <p className="text-[9px] text-text-muted italic">{hoveredFinding.subtitle}</p>}
          {hoveredFinding.detail && <p className="text-[9px] text-text-muted">{hoveredFinding.detail}</p>}
          <p className="text-[8px] text-text-muted mt-0.5">chr{hoveredFinding.chromosome} · {hoveredFinding.significance}</p>
        </div>
      )}
    </div>
  );
}
