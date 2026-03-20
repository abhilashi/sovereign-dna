export const colors = {
  surface: '#FAFAF8',
  border: '#E8E6E3',
  text: '#1A1A1A',
  textMuted: '#6B6965',
  riskLow: '#4A7C59',
  riskModerate: '#C4A35A',
  riskElevated: '#C17829',
  riskHigh: '#A94442',
  accent: '#2D5F8A',
  ancestry: {
    ochre: '#C4953A',
    terracotta: '#C27849',
    forest: '#4A7C59',
    slate: '#5B7B94',
    rust: '#A94442',
  },
} as const;

export const riskColors = {
  low: colors.riskLow,
  moderate: colors.riskModerate,
  elevated: colors.riskElevated,
  high: colors.riskHigh,
} as const;

export type RiskLevel = 'low' | 'moderate' | 'elevated' | 'high';

export function getRiskColor(level: RiskLevel): string {
  return riskColors[level];
}
