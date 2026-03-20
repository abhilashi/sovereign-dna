import Card from './Card';
import Sparkline from './Sparkline';

interface MetricCardProps {
  label: string;
  value: string | number;
  unit?: string;
  change?: number;
  sparklineData?: number[];
  description?: string;
}

export default function MetricCard({
  label,
  value,
  unit,
  change,
  sparklineData,
  description,
}: MetricCardProps) {
  return (
    <Card>
      <div className="flex items-start justify-between">
        <div>
          <p className="text-xs uppercase tracking-wider text-text-muted font-semibold mb-1">
            {label}
          </p>
          <div className="flex items-baseline gap-1.5">
            <span className="text-2xl font-light text-text font-mono">
              {typeof value === 'number' ? value.toLocaleString() : value}
            </span>
            {unit && (
              <span className="text-sm text-text-muted">{unit}</span>
            )}
          </div>
          {change !== undefined && (
            <p
              className={`text-xs mt-1 font-mono ${
                change > 0
                  ? 'text-risk-low'
                  : change < 0
                    ? 'text-risk-high'
                    : 'text-text-muted'
              }`}
            >
              {change > 0 ? '+' : ''}
              {change.toFixed(1)}%
            </p>
          )}
          {description && (
            <p className="text-xs text-text-muted mt-2 leading-relaxed">
              {description}
            </p>
          )}
        </div>
        {sparklineData && sparklineData.length > 1 && (
          <div className="ml-4 mt-1">
            <Sparkline data={sparklineData} />
          </div>
        )}
      </div>
    </Card>
  );
}
