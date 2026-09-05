import { render, screen } from '@testing-library/react';
import { ThemeProvider } from '@mui/material';
import { MiniMetricChart, ServerMetricCard } from '@/features/activity/pages/ActivityPage';
import { createAppTheme } from '@/theme/theme';

describe('dashboard de atividade', () => {
  it('renderiza métricas e histórico curto em pt-BR nos temas claro e escuro', () => {
    for (const mode of ['light', 'dark'] as const) {
      const { unmount } = render(
        <ThemeProvider theme={createAppTheme(mode)}>
          <ServerMetricCard label="Memória" value="1,8 GB / 8 GB" />
          <MiniMetricChart title="CPU" field="cpuUsagePercent" suffix="%" points={[
            { capturedAt: '2026-08-30T10:00:00Z', cpuUsagePercent: 12, memoryUsagePercent: 30, activePlaybackSessions: 1 },
            { capturedAt: '2026-08-30T10:00:04Z', cpuUsagePercent: 24, memoryUsagePercent: 35, activePlaybackSessions: 2 },
          ]} />
        </ThemeProvider>,
      );
      expect(screen.getByText('Memória')).toBeInTheDocument();
      expect(screen.getByText('1,8 GB / 8 GB')).toBeInTheDocument();
      expect(screen.getByText('24.0%')).toBeInTheDocument();
      unmount();
    }
  });
});
