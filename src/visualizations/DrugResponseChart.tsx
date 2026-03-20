import { colors } from '../design-system/tokens';

interface EnzymeData {
  gene: string;
  phenotype: string;
  starAllele: string;
}

interface DrugResponseChartProps {
  data: EnzymeData[];
}

const STATUS_POSITIONS: Record<string, number> = {
  poor: 0,
  intermediate: 1,
  normal: 2,
  'ultra-rapid': 3,
};

const STATUS_LABELS = ['Poor', 'Intermediate', 'Normal', 'Ultra-rapid'];

const STATUS_COLORS: Record<string, string> = {
  'ultra-rapid': colors.accent,
  normal: colors.riskLow,
  intermediate: colors.riskModerate,
  poor: colors.riskHigh,
};

export default function DrugResponseChart({ data }: DrugResponseChartProps) {
  const rowHeight = 32;
  const labelWidth = 100;
  const chartWidth = 300;
  const rightMargin = 80;
  const totalWidth = labelWidth + chartWidth + rightMargin;
  const totalHeight = data.length * rowHeight + 40;

  const segmentWidth = chartWidth / 4;

  return (
    <svg
      width="100%"
      height={totalHeight}
      viewBox={`0 0 ${totalWidth} ${totalHeight}`}
      className="block"
      preserveAspectRatio="xMinYMin meet"
    >
      {/* Column headers */}
      {STATUS_LABELS.map((label, i) => (
        <text
          key={label}
          x={labelWidth + i * segmentWidth + segmentWidth / 2}
          y={12}
          textAnchor="middle"
          fill="#6B6965"
          fontSize="8"
          fontFamily="'Inter', sans-serif"
        >
          {label}
        </text>
      ))}

      {/* Divider line */}
      <line
        x1={labelWidth}
        x2={labelWidth + chartWidth}
        y1={20}
        y2={20}
        stroke="#E8E6E3"
        strokeWidth={1}
      />

      {data.map((enzyme, i) => {
        const y = i * rowHeight + 36;
        const pos = STATUS_POSITIONS[enzyme.phenotype] ?? 2;
        const dotX = labelWidth + pos * segmentWidth + segmentWidth / 2;
        const dotColor = STATUS_COLORS[enzyme.phenotype] ?? '#6B6965';

        return (
          <g key={enzyme.gene}>
            {/* Row background */}
            {i % 2 === 1 && (
              <rect
                x={0}
                y={y - rowHeight / 2 + 4}
                width={totalWidth}
                height={rowHeight}
                fill="#F5F5F3"
              />
            )}

            {/* Enzyme label */}
            <text
              x={0}
              y={y + 4}
              fill="#1A1A1A"
              fontSize="10"
              fontFamily="'JetBrains Mono', monospace"
            >
              {enzyme.gene}
            </text>

            {/* Track line */}
            <line
              x1={labelWidth}
              x2={labelWidth + chartWidth}
              y1={y + 4}
              y2={y + 4}
              stroke="#E8E6E3"
              strokeWidth={1}
            />

            {/* Position markers */}
            {[0, 1, 2, 3].map((p) => (
              <circle
                key={p}
                cx={labelWidth + p * segmentWidth + segmentWidth / 2}
                cy={y + 4}
                r={2}
                fill="#E8E6E3"
              />
            ))}

            {/* Active dot */}
            <circle
              cx={dotX}
              cy={y + 4}
              r={5}
              fill={dotColor}
            />

            {/* Star allele label */}
            <text
              x={labelWidth + chartWidth + 8}
              y={y + 4}
              dominantBaseline="central"
              fill="#6B6965"
              fontSize="9"
              fontFamily="'JetBrains Mono', monospace"
            >
              {enzyme.starAllele}
            </text>
          </g>
        );
      })}
    </svg>
  );
}
