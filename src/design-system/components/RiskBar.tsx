import { type RiskLevel, getRiskColor } from '../tokens';

interface RiskBarProps {
  level: RiskLevel;
  score: number;
  label: string;
  showValue?: boolean;
}

export default function RiskBar({ level, score, label, showValue = true }: RiskBarProps) {
  const color = getRiskColor(level);
  const clampedScore = Math.max(0, Math.min(1, score));

  return (
    <div className="flex items-center gap-3 py-1.5">
      <span className="text-sm text-text w-40 shrink-0 truncate">{label}</span>
      <div className="flex-1 h-1.5 bg-border rounded-full overflow-hidden">
        <div
          className="h-full rounded-full transition-all duration-500"
          style={{ width: `${clampedScore * 100}%`, backgroundColor: color }}
        />
      </div>
      {showValue && (
        <span
          className="text-xs font-mono w-12 text-right shrink-0"
          style={{ color }}
        >
          {(clampedScore * 100).toFixed(0)}%
        </span>
      )}
    </div>
  );
}
