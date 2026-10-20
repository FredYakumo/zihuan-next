export function formatRate(value?: number): string {
  return value === undefined ? "—" : `${value.toFixed(1)} Tokens/s`;
}

export function formatPercent(value?: number): string {
  return value === undefined ? "—" : `${(value * 100).toFixed(1)}%`;
}
