#!/usr/bin/env node
// Teste de ponta a ponta dos terminais: sobe o binário de verdade e fala HTTP/WebSocket com ele.
// Uso: node scripts/e2e-terminals.mjs   (precisa de `cargo build --release` e Node 22+)
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, statSync } from 'node:fs';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const BIN = new URL('../target/release/wt-monitor', import.meta.url).pathname;
const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'wt-e2e-')));
const root = join(tmp, 'projects');
const wt = join(root, 'acme', 'api');
mkdirSync(join(wt, '.git'), { recursive: true });
mkdirSync(join(root, 'not-a-worktree'), { recursive: true });
const home = join(tmp, 'home');
mkdirSync(home); // HOME vazio: nenhum rc do usuário interfere no shell de teste

const tokenFile = join(tmp, 'token'); // hermético: nunca toca o token real do usuário
const servers = [];
function start(port, ...flags) {
  const p = spawn(BIN, [root, String(port), ...flags], { env: { ...process.env, HOME: home, SHELL: '/bin/bash', PS1: '', WT_TERMINALS: '', WT_TOKEN_FILE: tokenFile }, stdio: 'ignore' });
  servers.push(p);
  return p;
}
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function ready(port) {
  for (let i = 0; i < 50; i++) { try { await fetch(`http://127.0.0.1:${port}/api`); return; } catch { await sleep(100); } }
  throw new Error('servidor não subiu');
}

let failed = 0;
const ok = (cond, name, extra = '') => { console.log(`${cond ? 'ok  ' : 'FAIL'} ${name}${cond ? '' : '  ' + extra}`); if (!cond) failed++; };

// requisição crua, para controlar Host e Origin (o fetch não deixa)
function raw(port, lines) {
  return new Promise(resolve => {
    const s = net.connect(port, '127.0.0.1', () => s.write(lines.join('\r\n') + '\r\n\r\n'));
    let out = '';
    s.on('data', d => { out += d; });
    s.on('close', () => resolve(out));
    setTimeout(() => { s.destroy(); resolve(out); }, 1500);
  });
}

// cliente WebSocket que acumula a saída do terminal
function client(port, id, token) {
  const ws = new WebSocket(`ws://127.0.0.1:${port}/ws/term/${id}`);
  ws.binaryType = 'arraybuffer';
  const c = { ws, out: '', exit: null, closed: false, opened: new Promise(r => ws.addEventListener('open', r)) };
  const dec = new TextDecoder();
  ws.addEventListener('message', e => {
    if (typeof e.data === 'string') { const j = JSON.parse(e.data); if ('exit' in j) c.exit = j.exit; }
    else c.out += dec.decode(e.data);
  });
  ws.addEventListener('close', () => { c.closed = true; });
  c.send = s => ws.send(new TextEncoder().encode(s));
  c.waitFor = async (re, ms = 4000) => { const t = Date.now(); while (Date.now() - t < ms) { if (re.test(c.out)) return true; await sleep(50); } return false; };
  c.opened.then(() => { if (token) ws.send(JSON.stringify({ token })); });
  return c;
}

try {
  // ---- terminais desligados por padrão
  const P0 = 18700 + Math.floor(Math.random() * 100), P1 = P0 + 100;
  start(P0);
  await ready(P0);
  let r = await fetch(`http://127.0.0.1:${P0}/api/terminals`);
  ok(r.status === 404, 'sem --terminals, /api/terminals responde 404', r.status);

  // ---- terminais ligados
  start(P1, '--terminals');
  await ready(P1);
  const base = `http://127.0.0.1:${P1}`;
  const T = readFileSync(tokenFile, 'utf8').trim();
  ok(/^[0-9a-f]{64}$/.test(T) && (statSync(tokenFile).mode & 0o777) === 0o600, 'o token fica num arquivo 0600 criado na primeira execução');
  const anon = await (await fetch(`${base}/api/terminals`)).json();
  ok(anon.enabled === true && anon.auth === false && !('token' in anon) && !('terms' in anon), 'sem token, a API não revela token nem terminais');
  const authedInfo = await (await fetch(`${base}/api/terminals`, { headers: { 'X-WT': '1', 'X-WT-Token': T } })).json();
  ok(authedInfo.auth === true && Array.isArray(authedInfo.terms), 'com o token, a API lista os terminais');
  const wrong = await (await fetch(`${base}/api/terminals`, { headers: { 'X-WT': '1', 'X-WT-Token': 'x'.repeat(64) } })).json();
  ok(wrong.auth === false, 'com um token errado, a API responde como anônimo');
  const post = (body, headers = {}) => fetch(`${base}/api/terminals`, { method: 'POST', body, headers: { 'X-WT': '1', 'X-WT-Token': T, ...headers } });
  const q = (path, kind = 'shell', extra = '') => `path=${encodeURIComponent(path)}&kind=${kind}${extra}`;

  ok((await fetch(`${base}/api/terminals`, { method: 'POST', body: q(wt) })).status === 403, 'POST sem cabeçalhos é recusado');
  ok((await post(q(wt), { 'X-WT-Token': 'errado' })).status === 403, 'POST com token errado é recusado');
  ok((await post(q(wt), { 'X-WT': '' })).status === 403, 'POST sem X-WT é recusado');
  ok((await post(q('/etc'))).status === 400, 'caminho fora da raiz é recusado');
  ok((await post(q(join(root, 'not-a-worktree')))).status === 400, 'pasta sem .git é recusada');
  ok((await post(q(join(wt, '..', '..', '..')))).status === 400, 'caminho com .. é recusado');
  ok((await post(q(wt, 'rm -rf'))).status === 400, 'kind arbitrário é recusado (só shell|claude)');

  r = await post(q(wt, 'shell', '&cols=100&rows=30'));
  const { id } = await r.json();
  ok(r.status === 201 && id > 0, 'cria um terminal no worktree', r.status);

  // ---- WebSocket: proteções
  const hs = (extra) => ['GET /ws/term/' + id + ' HTTP/1.1', `Host: 127.0.0.1:${P1}`, 'Upgrade: websocket', 'Connection: Upgrade', 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==', 'Sec-WebSocket-Version: 13', ...extra];
  ok((await raw(P1, hs(['Origin: http://evil.example']))).startsWith('HTTP/1.1 403'), 'WebSocket de Origin estranha é recusado');
  ok((await raw(P1, hs([`Origin: http://127.0.0.1:${P1}`]))).startsWith('HTTP/1.1 101'), 'WebSocket da Origin do painel é aceito');
  ok((await raw(P1, ['GET /api HTTP/1.1', 'Host: evil.example'])).startsWith('HTTP/1.1 403'), 'Host estranho é recusado (DNS rebinding)');

  const bad = client(P1, id, 'token-errado');
  await bad.opened; await sleep(500);
  ok(bad.closed && bad.out === '', 'WebSocket com token errado é fechado sem enviar nada');

  // ---- sessão de verdade
  const c = client(P1, id, T);
  await c.opened;
  c.send('echo resultado-$((6*7))\n');
  ok(await c.waitFor(/resultado-42/), 'o shell executa comandos e a saída chega ao cliente', JSON.stringify(c.out.slice(-200)));
  c.send('pwd\n');
  ok(await c.waitFor(new RegExp(wt.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))), 'o shell começa na pasta do worktree', JSON.stringify(c.out.slice(-200)));
  c.send('stty size\n');
  ok(await c.waitFor(/30 100/), 'o tamanho inicial (linhas colunas) é aplicado', JSON.stringify(c.out.slice(-100)));
  c.ws.send(JSON.stringify({ resize: [132, 43] }));
  await sleep(150);
  c.send('stty size\n');
  ok(await c.waitFor(/43 132/), 'resize pelo WebSocket muda o tamanho do terminal', JSON.stringify(c.out.slice(-100)));

  // ---- reconexão: a saída acumulada é reenviada
  c.ws.close();
  await sleep(200);
  const c2 = client(P1, id, T);
  await c2.opened;
  ok(await c2.waitFor(/resultado-42/), 'ao reconectar, a saída anterior é reenviada');
  const listed = await (await fetch(`${base}/api/terminals`, { headers: { 'X-WT': '1', 'X-WT-Token': T } })).json();
  ok(listed.terms.length === 1 && listed.terms[0].alive && listed.terms[0].name === 'acme/api', 'o terminal aparece na lista, vivo', JSON.stringify(listed.terms));

  // ---- encerramento
  c2.send('exit 3\n');
  await sleep(800);
  ok(c2.exit === 3, 'ao sair, o cliente recebe {"exit":3}', String(c2.exit));
  const after = await (await fetch(`${base}/api/terminals`, { headers: { 'X-WT': '1', 'X-WT-Token': T } })).json();
  ok(after.terms[0] && after.terms[0].alive === false && after.terms[0].exit === 3, 'o terminal encerrado continua listado com o código de saída');
  r = await fetch(`${base}/api/terminals/${id}`, { method: 'DELETE', headers: { 'X-WT': '1', 'X-WT-Token': T } });
  ok(r.status === 204, 'DELETE remove o terminal', r.status);
  ok((await fetch(`${base}/api/terminals/${id}`, { method: 'DELETE', headers: { 'X-WT': '1' } })).status === 403, 'DELETE sem token é recusado');

  // ---- DELETE de um terminal vivo mata o processo
  const id2 = (await (await post(q(wt))).json()).id;
  const c3 = client(P1, id2, T); await c3.opened;
  c3.send('echo $$ > pid.txt; sleep 300\n'); await sleep(400);
  r = await fetch(`${base}/api/terminals/${id2}`, { method: 'DELETE', headers: { 'X-WT': '1', 'X-WT-Token': T } });
  await sleep(600);
  ok(r.status === 204 && c3.exit !== null, 'DELETE de um terminal vivo o encerra (cliente recebe exit)', `status=${r.status} exit=${c3.exit}`);

  // ---- limite de terminais
  let last = 201;
  for (let i = 0; i < 17; i++) last = (await post(q(wt))).status;
  ok(last === 429, 'há um limite de 16 terminais simultâneos', String(last));
} catch (e) {
  console.error('erro no teste:', e);
  failed++;
} finally {
  servers.forEach(p => p.kill('SIGKILL'));
  rmSync(tmp, { recursive: true, force: true });
}
console.log(failed ? `\n${failed} falha(s)` : '\ntodos os testes passaram');
process.exit(failed ? 1 : 0);
