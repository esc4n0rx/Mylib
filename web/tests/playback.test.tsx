import { fireEvent, render, screen } from '@testing-library/react';
import { PlayerControls } from '@/features/playback/components/PlayerControls';
import { browserCapabilities } from '@/features/playback/session';

describe('controles do player', () => {
  it('informa ao servidor um limite de áudio compatível com MediaSource', () => {
    expect(browserCapabilities().maxAudioChannels).toBe(2);
  });
  it('aciona play, saltos, mute, fullscreen e qualidade em pt-BR', () => {
    const toggle = vi.fn(); const skip = vi.fn(); const mute = vi.fn(); const fullscreen = vi.fn(); const quality = vi.fn();
    render(<PlayerControls playing={false} current={65} duration={120} volume={.7} muted={false} fullscreen={false} quality="AUTO" qualities={['AUTO', '720P']} title="Filme" onToggle={toggle} onSeek={vi.fn()} onSkip={skip} onVolume={vi.fn()} onMute={mute} onFullscreen={fullscreen} onQuality={quality} onBack={vi.fn()} onStats={vi.fn()} />);
    fireEvent.click(screen.getByLabelText('Reproduzir'));
    fireEvent.click(screen.getByLabelText('Voltar 10 segundos'));
    fireEvent.click(screen.getByLabelText('Avançar 10 segundos'));
    fireEvent.click(screen.getByLabelText('Silenciar'));
    fireEvent.click(screen.getByLabelText('Tela cheia'));
    expect(toggle).toHaveBeenCalled(); expect(skip).toHaveBeenNthCalledWith(1, -10); expect(skip).toHaveBeenNthCalledWith(2, 10); expect(mute).toHaveBeenCalled(); expect(fullscreen).toHaveBeenCalled();
    expect(screen.getByText('Automático')).toBeInTheDocument();
  });
});
