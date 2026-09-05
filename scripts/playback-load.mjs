// Usage: MYLIB_URL=http://localhost:8096 MYLIB_TOKEN=... MYLIB_DIRECT_URL='/api/v1/playback/.../direct?token=...' node scripts/playback-load.mjs
const base = process.env.MYLIB_URL ?? 'http://localhost:8096';
const direct = process.env.MYLIB_DIRECT_URL;
if (!direct) throw new Error('Defina MYLIB_DIRECT_URL com uma URL de sessão Direct Play válida.');

for (const clients of [10, 25, 50, 100]) {
  const started = performance.now();
  const results = await Promise.all(Array.from({ length: clients }, async (_, index) => {
    const offset = index * 1024;
    const response = await fetch(`${base}${direct}`, { headers: { Range: `bytes=${offset}-${offset + 1023}` } });
    await response.arrayBuffer();
    return response.status;
  }));
  const elapsed = performance.now() - started;
  const ok = results.filter(status => status === 206).length;
  console.log(JSON.stringify({ clients, ok, failed: clients - ok, elapsedMs: Math.round(elapsed), requestsPerSecond: Math.round(clients / elapsed * 1000) }));
}

console.log('Para transcode compartilhado, inicie três sessões com o mesmo mediaFileId/perfil e confirme uma única chave de pipeline em GET /api/v1/playback/sessions.');
