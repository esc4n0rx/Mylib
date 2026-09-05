import {
  Alert,
  Box,
  Button,
  CircularProgress,
  Snackbar,
  Stack,
  Typography,
} from '@mui/material';
import Hls from 'hls.js';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { api, type PlaybackQuality } from '@/api';
import {
  beginPlayback,
  loadPlayback,
  savePlayback,
  type StoredPlayback,
} from '../session';
import { PlayerControls } from '../components/PlayerControls';
import { PlayerStats } from '../components/PlayerStats';
import { useQueryClient } from '@tanstack/react-query';

export default function PlayerPage() {
  const { sessionId = '' } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const video = useRef<HTMLVideoElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const [stored, setStored] = useState<StoredPlayback | undefined>(() =>
    loadPlayback(sessionId),
  );
  const [playing, setPlaying] = useState(false);
  const [current, setCurrent] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [buffering, setBuffering] = useState(true);
  const [stats, setStats] = useState(false);
  const [error, setError] = useState('');
  const [nextCountdown, setNextCountdown] = useState<number>();
  const bufferEvents = useRef(0);
  const report = useCallback(
    (state: 'PLAYING' | 'PAUSED' | 'ENDED') => {
      const el = video.current;
      if (!stored || !el) return;
      void api.playback
        .progress(stored.response.sessionId, {
          positionMs: Math.round(el.currentTime * 1000),
          durationMs: Math.round((el.duration || stored.response.duration / 1000) * 1000),
          state,
          bufferEvents: bufferEvents.current,
        })
        .then((response) => { if (response.recommendationsInvalidated) void queryClient.invalidateQueries({ queryKey: ['recommendations'] }); })
        .catch(() => {});
      bufferEvents.current = 0;
    },
    [stored, queryClient],
  );

  useEffect(() => {
    if (!stored) return;
    const el = video.current!;
    let hls: Hls | undefined;
    let restartTimer: ReturnType<typeof setTimeout> | undefined;
    let destroyed = false;
    let desiredPosition = Math.max(0, stored.response.resumePosition / 1000);
    let networkRecoveries = 0;
    let decoderRecoveries = 0;
    let mediaSourceRestarts = 0;
    setBuffering(true);
    setError('');

    const fail = (details: string) => {
      console.error('HLS fatal error', details);
      setError(`Não foi possível carregar o stream (${details}). Tente novamente.`);
    };
    const ready = () => {
      if (Number.isFinite(desiredPosition)) el.currentTime = desiredPosition;
      setDuration(el.duration || stored.response.duration / 1000);
      void el.play().catch(() => {});
    };
    const createHls = () => {
      if (destroyed) return;
      const instance = new Hls({
          maxBufferLength: 30,
          backBufferLength: 30,
          manifestLoadingMaxRetry: 6,
          manifestLoadingRetryDelay: 500,
          levelLoadingMaxRetry: 6,
          fragLoadingMaxRetry: 6,
        });
      hls = instance;
      instance.on(Hls.Events.MEDIA_ATTACHED, () => instance.loadSource(stored.response.streamUrl));
      instance.on(Hls.Events.ERROR, (_e, data) => {
          if (!data.fatal || instance !== hls) return;
          if (data.type === Hls.ErrorTypes.NETWORK_ERROR && networkRecoveries < 3) {
            networkRecoveries++;
            instance.startLoad();
            return;
          }
          if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
            const requiresFreshMediaSource = data.details === 'mediaSourceRequiresReset' || data.details === 'bufferAppendError' || data.details === 'bufferAppendingError';
            if (requiresFreshMediaSource && mediaSourceRestarts < 2) {
              mediaSourceRestarts++;
              desiredPosition = Number.isFinite(el.currentTime) ? el.currentTime : desiredPosition;
              console.warn('HLS MediaSource reset', data.details, mediaSourceRestarts);
              instance.destroy();
              hls = undefined;
              el.pause();
              el.removeAttribute('src');
              el.load();
              setBuffering(true);
              restartTimer = setTimeout(createHls, 250 * mediaSourceRestarts);
              return;
            }
            if (decoderRecoveries === 0) {
              decoderRecoveries++;
              instance.recoverMediaError();
              return;
            }
            if (decoderRecoveries === 1) {
              decoderRecoveries++;
              instance.swapAudioCodec();
              instance.recoverMediaError();
              return;
            }
          }
          fail(data.details);
        });
      instance.attachMedia(el);
    };

    el.addEventListener('loadedmetadata', ready);
    if (stored.response.streamUrl.includes('.m3u8')) {
      if (Hls.isSupported()) {
        createHls();
      } else {
        el.src = stored.response.streamUrl;
      }
    } else {
      el.src = stored.response.streamUrl;
    }
    return () => {
      destroyed = true;
      if (restartTimer) clearTimeout(restartTimer);
      hls?.destroy();
      el.removeEventListener('loadedmetadata', ready);
    };
  }, [stored]);
  useEffect(() => {
    const timer = setInterval(() => report('PLAYING'), 15000);
    return () => clearInterval(timer);
  }, [report]);
  useEffect(() => {
    const keydown = (e: KeyboardEvent) => {
      if (['INPUT', 'SELECT'].includes((e.target as HTMLElement).tagName)) return;
      const el = video.current;
      if (!el) return;
      switch (e.key.toLowerCase()) {
        case ' ':
          e.preventDefault();
          if (el.paused) void el.play();
          else el.pause();
          break;
        case 'arrowleft':
          el.currentTime = Math.max(0, el.currentTime - 10);
          break;
        case 'arrowright':
          el.currentTime = Math.min(el.duration, el.currentTime + 10);
          break;
        case 'arrowup':
          e.preventDefault();
          el.volume = Math.min(1, el.volume + 0.1);
          break;
        case 'arrowdown':
          e.preventDefault();
          el.volume = Math.max(0, el.volume - 0.1);
          break;
        case 'm':
          el.muted = !el.muted;
          break;
        case 'f':
          void root.current?.requestFullscreen();
          break;
      }
    };
    document.addEventListener('keydown', keydown);
    const full = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener('fullscreenchange', full);
    return () => {
      document.removeEventListener('keydown', keydown);
      document.removeEventListener('fullscreenchange', full);
    };
  }, []);
  useEffect(() => {
    const close = () => {
      if (stored) void api.playback.stop(stored.response.sessionId);
    };
    window.addEventListener('pagehide', close);
    return () => window.removeEventListener('pagehide', close);
  }, [stored]);

  const changeQuality = async (quality: PlaybackQuality) => {
    if (!stored || !video.current) return;
    const position = Math.round(video.current.currentTime * 1000);
    try {
      const next = await beginPlayback({
        mediaItemId: stored.request.mediaItemId,
        mediaFileId: stored.request.mediaFileId,
        episodeId: stored.request.episodeId,
        quality,
        resumeFrom: position,
      });
      await api.playback.stop(stored.response.sessionId);
      savePlayback(next);
      setStored(next);
    } catch {
      setError('Não foi possível trocar a qualidade.');
    }
  };
  const playNext = useCallback(async () => {
    const next = stored?.response.nextEpisode;
    if (!stored || !next) return;
    try {
      const value = await beginPlayback({
        mediaItemId: stored.request.mediaItemId,
        mediaFileId: next.mediaFileId,
        episodeId: next.episodeId,
      });
      await api.playback.stop(stored.response.sessionId);
      navigate(`/player/${value.response.sessionId}`, { replace: true });
      setStored(value);
      setNextCountdown(undefined);
    } catch {
      setError('Não foi possível iniciar o próximo episódio.');
    }
  }, [navigate, stored]);
  useEffect(() => {
    if (nextCountdown === undefined) return;
    if (nextCountdown <= 0) {
      void playNext();
      return;
    }
    const timer = setTimeout(() => setNextCountdown(nextCountdown - 1), 1000);
    return () => clearTimeout(timer);
  }, [nextCountdown, playNext]);

  if (!stored)
    return (
      <Stack
        alignItems="center"
        justifyContent="center"
        sx={{ height: '100vh', bgcolor: '#000', color: '#fff' }}
        spacing={2}
      >
        <Typography variant="h2">Sessão de reprodução expirada</Typography>
        <Button variant="contained" onClick={() => navigate(-1)}>
          Voltar
        </Button>
      </Stack>
    );
  const episode = stored.response.content.episodeNumber
    ? `T${stored.response.content.seasonNumber} · E${stored.response.content.episodeNumber} · ${stored.response.content.episodeName ?? ''}`
    : undefined;
  const leave = async () => {
    const el = video.current;
    if (el) {
      await api.playback
        .progress(stored.response.sessionId, {
          positionMs: Math.round(el.currentTime * 1000),
          durationMs: Math.round((el.duration || stored.response.duration / 1000) * 1000),
          state: 'PAUSED',
        })
        .then((response) => { if (response.recommendationsInvalidated) void queryClient.invalidateQueries({ queryKey: ['recommendations'] }); })
        .catch(() => undefined);
    }
    await api.playback.stop(stored.response.sessionId).catch(() => undefined);
    navigate(-1);
  };
  return (
    <Box
      ref={root}
      sx={{
        position: 'fixed',
        inset: 0,
        bgcolor: '#000',
        zIndex: 1400,
        overflow: 'hidden',
      }}
    >
      <Box
        component="video"
        ref={video}
        playsInline
        onPlay={() => setPlaying(true)}
        onPause={() => {
          setPlaying(false);
          report('PAUSED');
        }}
        onTimeUpdate={(e) => setCurrent(e.currentTarget.currentTime)}
        onDurationChange={(e) => setDuration(e.currentTarget.duration)}
        onVolumeChange={(e) => {
          setVolume(e.currentTarget.volume);
          setMuted(e.currentTarget.muted);
        }}
        onWaiting={() => {
          setBuffering(true);
          bufferEvents.current++;
        }}
        onPlaying={() => setBuffering(false)}
        onSeeked={() => report('PLAYING')}
        onEnded={() => {
          report('ENDED');
          if (
            stored.response.nextEpisode &&
            localStorage.getItem('mylib.autoNext') !== 'false'
          )
            setNextCountdown(10);
        }}
        sx={{ width: '100%', height: '100%', objectFit: 'contain' }}
      />
      {buffering ? (
        <Stack
          sx={{ position: 'absolute', inset: 0, pointerEvents: 'none' }}
          alignItems="center"
          justifyContent="center"
          spacing={1}
        >
          <CircularProgress />
          <Typography color="white">Carregando…</Typography>
        </Stack>
      ) : null}
      <PlayerControls
        playing={playing}
        current={current}
        duration={duration}
        volume={volume}
        muted={muted}
        fullscreen={fullscreen}
        quality={stored.response.quality}
        qualities={stored.response.availableQualities}
        title={stored.response.content.title}
        episode={episode}
        onToggle={() =>
          video.current?.paused ? void video.current.play() : video.current?.pause()
        }
        onSeek={(v) => {
          if (video.current) video.current.currentTime = v;
        }}
        onSkip={(v) => {
          if (video.current)
            video.current.currentTime = Math.max(
              0,
              Math.min(video.current.duration, video.current.currentTime + v),
            );
        }}
        onVolume={(v) => {
          if (video.current) {
            video.current.volume = v;
            video.current.muted = false;
          }
        }}
        onMute={() => {
          if (video.current) video.current.muted = !video.current.muted;
        }}
        onFullscreen={() =>
          fullscreen
            ? void document.exitFullscreen()
            : void root.current?.requestFullscreen()
        }
        onQuality={(v) => void changeQuality(v)}
        onBack={() => void leave()}
        onStats={() => setStats(true)}
      />
      <PlayerStats
        open={stats}
        onClose={() => setStats(false)}
        session={stored.response}
      />
      {nextCountdown !== undefined ? (
        <Box
          sx={{
            position: 'absolute',
            right: { xs: 16, sm: 32 },
            left: { xs: 16, sm: 'auto' },
            bottom: { xs: 96, sm: 120 },
            p: { xs: 2, sm: 3 },
            borderRadius: 2,
            bgcolor: 'rgba(15,15,15,.88)',
            color: '#fff',
          }}
        >
          <Typography variant="h3">Próximo episódio em {nextCountdown}…</Typography>
          <Stack direction="row" spacing={1} sx={{ mt: 2 }}>
            <Button variant="contained" onClick={() => void playNext()}>
              Reproduzir agora
            </Button>
            <Button color="inherit" onClick={() => setNextCountdown(undefined)}>
              Cancelar
            </Button>
          </Stack>
        </Box>
      ) : null}
      <Snackbar open={Boolean(error)} onClose={() => setError('')}>
        <Alert severity="error">{error}</Alert>
      </Snackbar>
    </Box>
  );
}
