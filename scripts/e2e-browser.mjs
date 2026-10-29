#!/usr/bin/env node
// Teste de ponta a ponta no navegador real: Chrome headless controlado pelo protocolo de depuração (CDP).
// Sobe o servidor com --terminals, abre a interface, cria terminais, digita, recarrega e confere o resultado.
// Uso: node scripts/e2e-browser.mjs   (precisa de `cargo build --release`, Node 22+ e Chrome/Chromium)
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const BIN = new URL('../target/release/wt-monitor', import.meta.url).pathname;
const which = n => { try { return execFileSync('which', [n], { encoding: 'utf8' }).trim(); } catch { return ''; } };
const CHROME = ['google-chrome', 'chromium', 'chromium-browser'].map(which).find(Boolean);
if (!CHROME) { console.log('Chrome/Chromium não encontrado: teste de navegador ignorado'); process.exit(0); }

const tmp = realpathSync(mkdtempSync(join(tmpdir(), 'wt-browser-')));
const root = join(tmp, 'projects'), wt = join(root, 'acme', 'api'), home = join(tmp, 'home');
mkdirSync(join(wt, '.git'), { recursive: true });
mkdirSync(home);

const tokenFile = join(tmp, 'token'); // hermético: nunca toca o token real do usuário
const PORT = 18900 + Math.floor(Math.random() * 50), DBG = PORT + 100;
const procs = [];
const sleep = ms => new Promise(r => setTimeout(r, ms));
let failed = 0;
const ok = (cond, name, extra = '') => { console.log(`${cond ? 'ok  ' : 'FAIL'} ${name}${cond ? '' : '  ' + extra}`); if (!cond) failed++; };

class CDP {
  constructor(url) { this.ws = new WebSocket(url); this.n = 0; this.wait = new Map(); this.errors = []; this.ws.onmessage = e => this.on(JSON.parse(e.data)); this.open = new Promise(r => { this.ws.onopen = r; }); }
  on(m) {
    if (m.id && this.wait.has(m.id)) { this.wait.get(m.id)(m); this.wait.delete(m.id); return; }
    if (m.method === 'Runtime.exceptionThrown') this.errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
    if (m.method === 'Log.entryAdded' && m.params.entry.level === 'error' && !/favicon/.test(m.params.entry.url || '')) this.errors.push(m.params.entry.text);
  }
  send(method, params = {}) { const id = ++this.n; this.ws.send(JSON.stringify({ id, method, params })); return new Promise(r => this.wait.set(id, r)); }
  async eval(expression) {
    const { result } = await this.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || 'erro no avaliador');
    return result.result.value;
  }
  async until(expression, ms = 8000) { const t = Date.now(); while (Date.now() - t < ms) { try { const v = await this.eval(expression); if (v) return v; } catch { /* a página pode estar recarregando */ } await sleep(100); } return false; }
}
async function until(fn, ms = 6000) { const t = Date.now(); while (Date.now() - t < ms) { if (fn()) return true; await sleep(100); } return false; }
const bufferOf = id => `(()=>{const t=TERM.panes.get(${id}).xterm;let s='';for(let i=0;i<t.buffer.active.length;i++)s+=t.buffer.active.getLine(i).translateToString(true)+'\\n';return s})()`;

try {
  procs.push(spawn(BIN, [root, String(PORT), '--terminals', '--control'], { env: { ...process.env, HOME: home, SHELL: '/bin/bash', PS1: '', WT_TOKEN_FILE: tokenFile }, stdio: 'ignore' }));
  // um processo com cwd no worktree faz o worktree aparecer na lista do painel
  procs.push(spawn('sleep', ['600'], { cwd: wt, stdio: 'ignore' }));
  procs.push(spawn(CHROME, ['--headless=new', '--no-sandbox', '--disable-gpu', `--remote-debugging-port=${DBG}`, `--user-data-dir=${join(tmp, 'chrome')}`, '--window-size=1440,900', 'about:blank'], { stdio: 'ignore' }));
  let target;
  for (let i = 0; i < 60 && !target; i++) { try { target = await (await fetch(`http://127.0.0.1:${DBG}/json/new?about:blank`, { method: 'PUT' })).json(); } catch { await sleep(200); } }
  const c = new CDP(target.webSocketDebuggerUrl);
  await c.open;
  await c.send('Runtime.enable'); await c.send('Log.enable'); await c.send('Page.enable');
  // 1) sem o token: a aba aparece, mas os botões ficam desativados e há um aviso
  await c.send('Page.navigate', { url: `http://127.0.0.1:${PORT}/?mode=term` });
  ok(await c.until(`!document.getElementById('segterm').hidden && document.getElementById('tnew-shell').disabled && !document.getElementById('tnote').hidden`), 'sem token, os terminais ficam desativados e o painel explica como abrir');
  ok(await c.eval(`document.getElementById('tnote').textContent.includes('make open-terminals')`), 'o aviso aponta para "make open-terminals"');

  // 2) com o token no link: ele é guardado, o endereço é limpo e tudo funciona
  const token = readFileSync(tokenFile, 'utf8').trim();
  await c.send('Page.navigate', { url: `http://127.0.0.1:${PORT}/?mode=term#token=${token}` });
  ok(await c.until(`!document.getElementById('tnew-shell').disabled && location.hash === ''`), 'com #token no link, os botões ativam e o token some do endereço');
  ok(await c.until(`!document.getElementById('segterm').hidden && document.querySelectorAll('#twt option').length > 0`), 'a aba Terminais aparece e lista o worktree');
  ok(await c.until(`document.querySelector('#twt option').textContent === 'acme/api'`), 'o seletor traz o worktree acme/api');

  await c.eval(`document.getElementById('tnew-shell').click()`);
  ok(await c.until(`TERM.panes.size === 1 && [...TERM.panes.values()][0].status === 'conectado'`), 'clicar em "Novo terminal" abre e conecta um terminal');
  const id1 = await c.eval(`[...TERM.panes.keys()][0]`);
  await c.eval(`TERM.panes.get(${id1}).xterm.input('echo browser-$((6*7))\\r')`);
  ok(await c.until(`${bufferOf(id1)}.includes('browser-42')`), 'o comando digitado roda e a saída aparece no xterm');

  await c.eval(`TERM.panes.get(${id1}).xterm.focus()`);
  await c.send('Input.insertText', { text: 'echo teclado-$((7*6))' });
  await c.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r' });
  await c.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
  ok(await c.until(`${bufferOf(id1)}.includes('teclado-42')`), 'teclas reais (CDP) chegam ao shell');
  ok(await c.eval(`TERM.panes.get(${id1}).xterm.cols > 20`), 'o terminal foi ajustado ao tamanho do painel', String(await c.eval(`TERM.panes.get(${id1}).xterm.cols`)));

  await c.send('Page.reload');
  ok(await c.until(`TERM.panes.size === 1 && [...TERM.panes.values()][0].status === 'conectado'`), 'depois de recarregar a página, o terminal reaparece');
  const id1b = await c.eval(`[...TERM.panes.keys()][0]`);
  ok(await c.until(`${bufferOf(id1b)}.includes('browser-42')`), 'a saída anterior é reenviada ao reconectar');

  await c.eval(`document.getElementById('tnew-claude').click()`);
  ok(await c.until(`TERM.panes.size === 2`), '"Novo Claude" abre um segundo terminal');
  const id2 = await c.eval(`TERM.metas[1].id`);
  ok(await c.until(`${bufferOf(id2)}.includes('claude')`), 'o terminal "claude" recebe o comando claude');

  await c.eval(`document.querySelector('#tlayout [data-layout="2"]').click()`);
  ok(await c.until(`[...TERM.panes.values()].every(p => !p.el.hidden && p.xterm.cols > 10)`), 'o layout de 2 painéis mostra os dois terminais lado a lado');
  ok(!(await c.eval(`getComputedStyle(document.getElementById('tgrid')).gridTemplateColumns.split(' ').length < 2`)), 'a grade tem duas colunas');

  await c.eval(`document.querySelector('.ttab .x').click()`);
  await c.until(`TERM.panes.size === 1`); // espera o primeiro fechar antes de clicar de novo
  await c.eval(`document.querySelector('.ttab .x').click()`);
  ok(await c.until(`TERM.panes.size === 0 && !document.getElementById('tempty').hidden`), 'fechar as abas encerra os terminais e mostra o estado vazio');
  const list = await (await fetch(`http://127.0.0.1:${PORT}/api/terminals`, { headers: { 'X-WT': '1', 'X-WT-Token': token } })).json();
  await sleep(500);
  ok(list.terms.length === 0, 'o servidor não lista mais terminais', JSON.stringify(list.terms));

  // 3) prioridade: a aba do drawer move processos de verdade (só se o sistema delega cgroups)
  const sleepPid = procs[1].pid, cgOf = pid => { try { return readFileSync(`/proc/${pid}/cgroup`, 'utf8'); } catch { return ''; } };
  await c.eval(`document.querySelector('[data-mode="sober"]').click()`);
  ok(await c.until(`data && data.groups.length === 1 && data.control && data.control.enabled`), 'o painel recebe o estado do controle e do worktree');
  if (await c.eval(`data.control.supported`)) {
    await c.eval(`openDrawer(data.groups[0].path, null); tab = 'prio'; renderDrawer()`);
    ok(await c.until(`!!document.querySelector('[data-cpu="baixa"]')`), 'a aba Prioridade aparece com os botões de nível');
    const origin = cgOf(sleepPid);
    await c.eval(`document.querySelector('[data-cpu="baixa"]').click()`);
    ok(await c.until(`data.groups[0].ctl.cpu === 'baixa' && document.querySelector('[data-cpu="baixa"]').getAttribute('aria-pressed') === 'true'`), 'clicar em "Baixa" muda o nível e o botão fica marcado');
    ok(await until(() => cgOf(sleepPid).includes('/wtm-')), 'o processo do worktree foi movido para o cgroup do painel');
    ok(await c.until(`document.querySelector('.row .prio') && document.querySelector('.row .prio').textContent.includes('baixa')`), 'a linha do worktree mostra a etiqueta "↓ baixa"');
    await c.eval(`document.querySelector('[data-high="2"]').click()`);
    ok(await c.until(`data.groups[0].ctl.high === 2 * 2 ** 30`), 'clicar em "2 GB" define o limite suave de memória');
    await c.eval(`document.querySelector('[data-cpu="normal"]').click(); document.querySelector('[data-high="0"]').click()`);
    ok(await c.until(`data.groups[0].ctl.cpu === 'normal' && data.groups[0].ctl.high === 0`), 'voltar a "Normal" e "Sem limite" limpa tudo');
    ok(await until(() => cgOf(sleepPid) === origin), 'o processo volta ao cgroup de origem', cgOf(sleepPid));
  }

  // 4) texto das conversas: markdown e tags viram HTML legível, e conteúdo malicioso nunca vira HTML
  const rich = t => c.eval(`richText(${JSON.stringify(t)})`);
  const evil = await rich('<img src=x onerror=alert(1)> <script>alert(1)</script> [x](javascript:alert(1)) **b** `<b>c</b>`');
  ok(!/<img|<script|href="javascript/i.test(evil) && evil.includes('<strong>b</strong>'), 'markdown vira HTML, mas HTML e links javascript: do texto nunca são interpretados', evil);
  ok((await rich('<task-notification><status>completed</status><summary>ok</summary></task-notification>')).includes('class="st ok"'), '<task-notification> vira um cartão com o status em destaque');
  ok((await rich('texto **truncado\n```ts\nconst x')).includes('<pre>') && (await rich('<task-notification><summary>cortado')).includes('class="xml"'), 'texto truncado no meio (bloco de código ou tag abertos) não quebra a renderização');
  ok(await c.eval(`plain('**Feito** em \`src/a.ts\` <summary>x</summary>') === 'Feito em src/a.ts x'`), 'as listas de uma linha mostram o texto sem marcações');

  await c.eval(`document.querySelector('[data-mode="sober"]').click()`);
  ok(await c.until(`!document.getElementById('overview').hidden && document.getElementById('terms').hidden`), 'voltar para a Visão geral esconde os terminais');
  ok(c.errors.length === 0, 'nenhum erro de JavaScript no console', c.errors.join(' | '));
} catch (e) {
  console.error('erro no teste:', e);
  failed++;
} finally {
  procs.forEach(p => p.kill('SIGKILL'));
  await sleep(300);
  rmSync(tmp, { recursive: true, force: true });
}
console.log(failed ? `\n${failed} falha(s)` : '\ntodos os testes passaram');
process.exit(failed ? 1 : 0);
