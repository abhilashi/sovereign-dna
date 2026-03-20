interface ChromosomeData {
  chromosome: string;
  count: number;
}

interface ChromosomeIdeogramProps {
  chromosomeData: ChromosomeData[];
}

export default function ChromosomeIdeogram({ chromosomeData }: ChromosomeIdeogramProps) {
  if (chromosomeData.length === 0) return null;

  const maxCount = Math.max(...chromosomeData.map((d) => d.count));

  const barHeight = 10;
  const rowHeight = 20;
  const labelWidth = 28;
  const maxBarWidth = 400;
  const svgWidth = labelWidth + maxBarWidth + 60;
  const svgHeight = chromosomeData.length * rowHeight + 4;

  return (
    <svg
      width="100%"
      height={svgHeight}
      viewBox={`0 0 ${svgWidth} ${svgHeight}`}
      className="block"
      preserveAspectRatio="xMinYMin meet"
    >
      {chromosomeData.map((d, i) => {
        const barWidth = (d.count / maxCount) * maxBarWidth;
        const opacity = 0.25 + (d.count / maxCount) * 0.75;
        const y = i * rowHeight + 2;
        return (
          <g key={d.chromosome}>
            <text
              x={labelWidth - 4}
              y={y + barHeight / 2}
              textAnchor="end"
              dominantBaseline="central"
              className="text-[9px] fill-text-muted"
              fontFamily="'JetBrains Mono', monospace"
            >
              {d.chromosome}
            </text>
            <rect
              x={labelWidth}
              y={y}
              width={barWidth}
              height={barHeight}
              rx={2}
              fill="#2D5F8A"
              opacity={opacity}
            />
            <text
              x={labelWidth + barWidth + 6}
              y={y + barHeight / 2}
              dominantBaseline="central"
              className="text-[8px] fill-text-muted"
              fontFamily="'JetBrains Mono', monospace"
            >
              {d.count.toLocaleString()}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
