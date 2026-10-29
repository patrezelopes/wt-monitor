#!/usr/bin/env node
// Teste de ponta a ponta do controle de prioridade: sobe o binário com --control e --terminals, move processos
// de verdade entre cgroups e confere /proc e o cgroupfs. Se o sistema não delega cgroups ao usuário, é ignorado.
// Uso: node scripts/e2e-control.mjs   (precisa de `cargo build --release` e Node 22+)
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const BIN = new URL('../target/release/wt-monitor', import.meta.url).pathname;
const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'wt-ctl-')));
const root = join(tmp, 'projects'), wt = join(root, 'acme', 'api'), other = join(root, 'acme', 'web'), home = join(tmp, 'home');
mkdirSync(join(wt, '.git'), { recursive: true });
mkdirSync(join(other, '.git'), { recursive: true });
mkdirSync(home);
const tokenFile = join(tmp, 'token'); // hermético: nunca toca o token real do usuário
const PORT = 19100 + Math.floor(Math.random() * 100), base = `http://127.0.0.1:${PORT}`;
const procs = [];
const sleep = ms => new Promise(r => setTimeout(r, ms));
let failed = 0;
const ok = (cond, name, extra = '') => { console.log(`${cond ? 'ok  ' : 'FAIL'} ${name}${cond ? '' : '  ' + extra}`); if (!cond) failed++; };
const T = () => readFileSync(tokenFile, 'utf8').trim();
const api = async () => (await fetch(`${base}/api`)).json();
const post = (path, body, token = T()) => fetch(`${base}${path}`, { method: 'POST', body, headers: { 'X-WT': '1', 'X-WT-Token': token } });
const cgOf = pid => { try { return readFileSync(`/proc/${pid}/cgroup`, 'utf8').trim().replace(/^0::/, ''); } catch { return ''; } };
const cgFile = (pid, f) => { try { return readFileSync(`/sys/fs/cgroup${cgOf(pid)}/${f}`, 'utf8').trim(); } catch { return ''; } };
async function until(fn, ms = 6000) { const t = Date.now(); while (Date.now() - t < ms) { if (await fn()) return true; await sleep(100); } return false; }
const grp = async path => (await api()).groups.find(g => g.path === path);

try {
  procs.push(spawn(BIN, [root, String(PORT), '--control', '--terminals'], { env: { ...process.env, HOME: home, SHELL: '/bin/bash', PS1: '', WT_TOKEN_FILE: tokenFile, WT_TERMINALS: '', WT_CONTROL: '' }, stdio: 'ignore' }));
  for (let i = 0; i < 50; i++) { try { await fetch(`${base}/api`); break; } catch { await sleep(100); } }
  // processos com cwd nos worktrees fazem os worktrees aparecerem
  const p1 = spawn('sleep', ['600'], { cwd: wt, stdio: 'ignore' }), p2 = spawn('sleep', ['600'], { cwd: other, stdio: 'ignore' });
  procs.push(p1, p2);
  const wtPath = wt;

  await until(async () => (await api()).groups.length === 2);
  const d = await api();
  ok(d.control && d.control.enabled === true, 'o painel informa que o controle está ligado');
  ok(typeof d.psi === 'object' && 'cpu' in d.psi && 'mem' in d.psi, 'a pressão do sistema (PSI) vem no /api');
  const g0 = d.groups.find(g => g.path === wtPath);
  ok(g0 && typeof g0.wait === 'number' && typeof g0.waiting === 'number', 'cada worktree traz a espera por CPU (wait, waiting)');
  if (!d.control.supported) { console.log(`controle indisponível neste sistema (${d.control.reason}): testes de cgroup ignorados`); }
  else {
    const origin1 = cgOf(p1.pid);
    const info = await (await fetch(`${base}/api/control`, { headers: { 'X-WT': '1', 'X-WT-Token': T() } })).json();
    ok(info.enabled && info.auth && info.supported, 'GET /api/control com token responde auth=true');
    ok((await (await fetch(`${base}/api/control`)).json()).auth === false, 'sem token, auth=false');

    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=baixa`, 'x'.repeat(64))).status === 403, 'POST com token errado é recusado');
    ok((await fetch(`${base}/api/control`, { method: 'POST', body: `path=${encodeURIComponent(wt)}&cpu=baixa` })).status === 403, 'POST sem cabeçalhos é recusado');
    ok((await post('/api/control', `path=%2Fetc&cpu=baixa`)).status === 400, 'caminho fora da raiz é recusado');
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=realtime`)).status === 400, 'nível desconhecido é recusado');
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&high=2000`)).status === 400, 'limite de memória acima de 1024 GiB é recusado');
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&high=abc`)).status === 400, 'limite de memória não numérico é recusado');
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}`)).status === 400, 'pedido vazio é recusado');
    await sleep(1300);
    ok(cgOf(p1.pid) === origin1, 'pedidos recusados não movem nenhum processo');

    // ---- prioridade baixa
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=baixa`)).status === 204, 'cpu=baixa é aceito');
    ok(await until(() => cgOf(p1.pid).includes('/wtm-')), 'o processo do worktree entra no cgroup do painel');
    ok(cgFile(p1.pid, 'cpu.weight') === '25', 'cpu.weight do grupo = 25 (baixa)', cgFile(p1.pid, 'cpu.weight'));
    ok(cgOf(p2.pid) === cgOf(p2.pid) && !cgOf(p2.pid).includes('/wtm-'), 'o outro worktree não é tocado');
    const g1 = await until(async () => (await grp(wtPath))?.ctl?.cpu === 'baixa') && await grp(wtPath);
    ok(g1 && g1.ctl.weight === 25 && g1.ctl.inside >= 1, 'o /api mostra o nível e quantos processos estão sob controle', JSON.stringify(g1?.ctl));

    // ---- memória
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&high=1&low=1`)).status === 204, 'high=1 e low=1 (GiB) são aceitos');
    ok(await until(() => cgFile(p1.pid, 'memory.high') === String(1 << 30)), 'memory.high = 1 GiB', cgFile(p1.pid, 'memory.high'));
    ok(cgFile(p1.pid, 'memory.low') === String(1 << 30), 'memory.low = 1 GiB', cgFile(p1.pid, 'memory.low'));
    ok(cgFile(p1.pid, 'memory.max') === 'max', 'o painel nunca define memory.max (nada é morto por limite)', cgFile(p1.pid, 'memory.max'));

    // ---- terminal nasce dentro do grupo
    const created = await post('/api/terminals', `path=${encodeURIComponent(wt)}&kind=shell`);
    const { id } = await created.json();
    const ws = new WebSocket(`ws://127.0.0.1:${PORT}/ws/term/${id}`);
    let out = '';
    ws.binaryType = 'arraybuffer';
    ws.addEventListener('message', e => { if (typeof e.data !== 'string') out += new TextDecoder().decode(e.data); });
    await new Promise(r => ws.addEventListener('open', r));
    ws.send(JSON.stringify({ token: T() }));
    await sleep(300);
    ws.send(new TextEncoder().encode('echo "shellpid=$$"; echo "shellcg=$(cat /proc/$$/cgroup)"\n'));
    ok(await until(() => /shellcg=0::.*\/wtm-/.test(out)), 'um terminal aberto pelo painel já nasce dentro do cgroup do worktree', JSON.stringify(out.slice(-200)));
    ws.close();
    await fetch(`${base}/api/terminals/${id}`, { method: 'DELETE', headers: { 'X-WT': '1', 'X-WT-Token': T() } });

    // ---- Foco
    ok((await post('/api/control/focus', `path=${encodeURIComponent(other)}`)).status === 204, 'ligar o Foco é aceito');
    ok(await until(() => cgFile(p2.pid, 'cpu.weight') === '1000'), 'o worktree em foco recebe peso 1000', cgFile(p2.pid, 'cpu.weight'));
    ok(await until(() => cgFile(p1.pid, 'cpu.weight') === '20'), 'os demais vão para segundo plano (peso 20)', cgFile(p1.pid, 'cpu.weight'));
    ok((await api()).control.focus === other, 'o /api informa onde está o Foco');
    ok((await post('/api/control/focus', 'path=')).status === 204, 'desligar o Foco é aceito');
    ok(await until(() => cgFile(p1.pid, 'cpu.weight') === '25'), 'sem Foco, o pedido original (baixa) volta', cgFile(p1.pid, 'cpu.weight'));
    ok(await until(() => !cgOf(p2.pid).includes('/wtm-')), 'o worktree que estava em foco volta ao cgroup de origem');

    // ---- o efeito de verdade: dois laços de CPU presos ao mesmo core, um por worktree
    const busy = cwd => spawn('taskset', ['-c', '0', 'sh', '-c', 'while :; do :; done'], { cwd, stdio: 'ignore' });
    const bA = busy(wt), bB = busy(other);
    procs.push(bA, bB);
    await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=normal&high=0&low=0`);
    await post('/api/control', `path=${encodeURIComponent(other)}&cpu=normal`);
    await sleep(4000);
    const even = { a: (await grp(wtPath)).cpu, b: (await grp(other)).cpu };
    ok(Math.abs(even.a - even.b) < 40, 'sem prioridade, os dois laços no mesmo core dividem a CPU', JSON.stringify(even));
    const waited = await grp(wtPath);
    ok(waited.wait > 25 && waited.waiting > 0.3, 'a medição enxerga a espera na fila (wait e waiting)', `wait=${waited.wait} waiting=${waited.waiting}`);
    await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=alta`);
    await post('/api/control', `path=${encodeURIComponent(other)}&cpu=baixa`);
    await sleep(5000);
    const skew = { a: (await grp(wtPath)).cpu, b: (await grp(other)).cpu };
    ok(skew.a > 70 && skew.b < 30, 'com alta x baixa, o worktree prioritário fica com quase todo o core', JSON.stringify(skew));
    const starved = await grp(other);
    ok(starved.wait > waited.wait - 5 && starved.wait > 60, 'quem foi rebaixado passa a ter a maior espera na fila', `wait=${starved.wait}`);
    bA.kill('SIGKILL'); bB.kill('SIGKILL');
    await post('/api/control', `path=${encodeURIComponent(other)}&cpu=normal`);

    // ---- voltar ao normal desfaz tudo
    const dir = `/sys/fs/cgroup${cgOf(p1.pid)}`;
    ok((await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=normal&high=0&low=0`)).status === 204, 'voltar ao normal é aceito');
    ok(await until(() => cgOf(p1.pid) === origin1), 'o processo volta ao cgroup de origem', cgOf(p1.pid));
    ok(await until(() => !existsSync(dir)), 'o cgroup do painel é removido quando não há mais nada a controlar');
  }
} catch (e) {
  console.error('erro no teste:', e);
  failed++;
} finally {
  // sai da prioridade antes de encerrar, para não deixar grupos para trás
  try { await post('/api/control', `path=${encodeURIComponent(wt)}&cpu=normal&high=0&low=0`); await post('/api/control', `path=${encodeURIComponent(other)}&cpu=normal&high=0&low=0`); await post('/api/control/focus', 'path='); await sleep(1500); } catch { /* servidor já caiu */ }
  procs.forEach(p => p.kill('SIGKILL'));
  await sleep(300);
  rmSync(tmp, { recursive: true, force: true });
}
console.log(failed ? `\n${failed} falha(s)` : '\ntodos os testes passaram');
process.exit(failed ? 1 : 0);
