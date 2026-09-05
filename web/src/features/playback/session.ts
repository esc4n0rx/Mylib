import { api, type PlaybackQuality, type PlaybackStartRequest, type PlaybackStartResponse } from '@/api';

export interface StoredPlayback { request: PlaybackStartRequest; response: PlaybackStartResponse }
const key = (id: string) => `mylib.playback.${id}`;

export function browserCapabilities(): PlaybackStartRequest['clientCapabilities'] {
  const video = document.createElement('video');
  const videoCodecs = ['h264'];
  if (video.canPlayType('video/mp4; codecs="hvc1"')) videoCodecs.push('hevc');
  return { containers: ['mp4', 'webm'], videoCodecs, audioCodecs: ['aac', 'mp3', 'opus', 'vorbis'], maxWidth: screen.width * devicePixelRatio, maxHeight: screen.height * devicePixelRatio, maxAudioChannels: 2, estimatedBandwidthKbps: navigator.connection?.downlink ? Math.round(navigator.connection.downlink * 1000) : undefined };
}

export async function beginPlayback(input: { mediaItemId: string; mediaFileId?: string; episodeId?: string; quality?: PlaybackQuality; resumeFrom?: number }) {
  const request: PlaybackStartRequest = { mediaItemId: input.mediaItemId, mediaFileId: input.mediaFileId, episodeId: input.episodeId, quality: input.quality ?? 'AUTO', clientCapabilities: browserCapabilities(), clientId: getClientId(), clientName: navigator.userAgent };
  const response = await api.playback.start(request);
  if (input.resumeFrom !== undefined) response.resumePosition = input.resumeFrom;
  const stored = { request, response };
  sessionStorage.setItem(key(response.sessionId), JSON.stringify(stored));
  return stored;
}

export function loadPlayback(id: string): StoredPlayback | undefined {
  try { const value = sessionStorage.getItem(key(id)); return value ? JSON.parse(value) as StoredPlayback : undefined; } catch { return undefined; }
}
export function savePlayback(value: StoredPlayback) { sessionStorage.setItem(key(value.response.sessionId), JSON.stringify(value)); }
function getClientId() { const storageKey = 'mylib.clientId'; let value = localStorage.getItem(storageKey); if (!value) { value = crypto.randomUUID(); localStorage.setItem(storageKey, value); } return value; }

declare global { interface Navigator { connection?: { downlink?: number } } }
