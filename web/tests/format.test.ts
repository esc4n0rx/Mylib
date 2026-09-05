import { describe, expect, it } from 'vitest';
import { formatBytes, formatNumber, formatPercent, formatDateTime } from '@/utils/format';

describe('pt-BR formatting', () => {
  it('groups thousands with a dot', () => {
    expect(formatNumber(1248)).toBe('1.248');
  });

  it('formats percentages with a comma decimal', () => {
    expect(formatPercent(62.4)).toBe('62,4%');
  });

  it('formats dates as dd/mm/yyyy hh:mm', () => {
    const out = formatDateTime('2026-08-28T22:18:00Z');
    expect(out).toMatch(/\d{2}\/\d{2}\/\d{4}/);
  });

  it('renders a dash for empty dates', () => {
    expect(formatDateTime(undefined)).toBe('—');
  });

  it('formats library sizes with pt-BR decimal separators', () => {
    expect(formatBytes(1536)).toBe('1,5 KB');
    expect(formatBytes(0)).toBe('0 B');
  });
});
