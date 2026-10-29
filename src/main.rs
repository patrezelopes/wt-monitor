//! Painel local de sessões por worktree. Uso: wt-monitor [raiz=~/dev] [porta=8765]
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TICK: f64 = 100.0; // CLK_TCK no Linux
const INDEX: &str = include_str!("../index.html");

struct Proc {
    pid: u32,
    wt: PathBuf,
    cmd: String,
    ticks: u64,
    cpu: u32,
    threads: u32,
    rss: u64,
    aff: String,
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o
}

fn worktree_of(
    cwd: &Path,
    root: &Path,
    cache: &mut HashMap<PathBuf, Option<PathBuf>>,
) -> Option<PathBuf> {
    if let Some(r) = cache.get(cwd) {
        return r.clone();
    }
    let mut found = None;
    for p in cwd.ancestors() {
        if !p.starts_with(root) {
            break;
        }
        if p.join(".git").exists() {
            found = Some(p.to_path_buf());
            break;
        }
    }
    cache.insert(cwd.to_path_buf(), found.clone());
    found
}

fn proc_info(pid: u32, root: &Path, cache: &mut HashMap<PathBuf, Option<PathBuf>>) -> Option<Proc> {
    let cwd = fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    if !cwd.starts_with(root) {
        return None;
    }
    let wt = worktree_of(&cwd, root, cache)?;
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rest: Vec<&str> = stat[stat.rfind(')')? + 2..].split_whitespace().collect();
    let n = |i: usize| rest.get(i).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let cmd = fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| {
            String::from_utf8_lossy(&b)
                .replace('\0', " ")
                .trim()
                .chars()
                .take(160)
                .collect()
        })
        .unwrap_or_default();
    let (mut rss, mut aff) = (0, String::new());
    for l in status.lines() {
        if let Some(v) = l.strip_prefix("VmRSS:") {
            rss = v
                .split_whitespace()
                .next()
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024;
        } else if let Some(v) = l.strip_prefix("Cpus_allowed_list:") {
            aff = v.trim().to_string();
        }
    }
    Some(Proc {
        pid,
        wt,
        cmd,
        ticks: n(11) + n(12),
        cpu: n(36) as u32,
        threads: n(17) as u32,
        rss,
        aff,
    })
}

/// (total, idle+iowait) por core
fn cpu_totals() -> BTreeMap<usize, (u64, u64)> {
    let mut out = BTreeMap::new();
    for l in fs::read_to_string("/proc/stat").unwrap_or_default().lines() {
        let mut it = l.split_whitespace();
        let Some(name) = it.next() else { continue };
        let Some(idx) = name
            .strip_prefix("cpu")
            .and_then(|d| d.parse::<usize>().ok())
        else {
            continue;
        };
        let v: Vec<u64> = it.filter_map(|x| x.parse().ok()).collect();
        if v.len() >= 5 {
            out.insert(idx, (v.iter().sum(), v[3] + v[4]));
        }
    }
    out
}

fn mem_info() -> (u64, u64) {
    let (mut total, mut avail) = (0, 0);
    for l in fs::read_to_string("/proc/meminfo")
        .unwrap_or_default()
        .lines()
    {
        let kb = |v: &str| {
            v.split_whitespace()
                .next()
                .and_then(|x| x.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024
        };
        if let Some(v) = l.strip_prefix("MemTotal:") {
            total = kb(v);
        } else if let Some(v) = l.strip_prefix("MemAvailable:") {
            avail = kb(v);
        }
    }
    (total, avail)
}

struct Group {
    cpu: f64,
    rss: u64,
    threads: u32,
    claude: bool,
    nproc: usize,
    cores: BTreeSet<u32>,
    aff: BTreeSet<String>,
    procs: Vec<(Proc, f64)>,
}

/// Limites de alerta, ajustáveis por variável de ambiente.
struct Cfg {
    cpu_pct: f64,
    cpu_secs: u64,
    mem_bytes: u64,
    sys_pct: f64,
}

impl Cfg {
    fn from_env() -> Self {
        let f = |k: &str, d: f64| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        Cfg {
            cpu_pct: f("WT_CPU_PCT", 300.0),
            cpu_secs: f("WT_CPU_SECS", 20.0) as u64,
            mem_bytes: (f("WT_MEM_GB", 4.0) * 1e9) as u64,
            sys_pct: f("WT_SYS_MEM_PCT", 90.0),
        }
    }
}

#[derive(Default)]
struct Entry {
    since: Option<Instant>,
    fired: bool,
    last: Option<Instant>,
    seen: bool,
}

#[derive(Default)]
struct Alerts {
    st: HashMap<String, Entry>,
    log: VecDeque<(f64, String)>,
}

/// Uma regra de alerta avaliada a cada amostra.
struct Rule {
    key: String,
    over: bool,
    hold: Duration,
    urgent: bool,
    title: &'static str,
    body: String,
}

impl Alerts {
    /// Dispara uma vez quando `over` se mantém por `hold`; rearma ao voltar ao normal (mín. 2 min entre disparos).
    fn check(&mut self, ts: f64, r: Rule) {
        let Rule {
            key,
            over,
            hold,
            urgent,
            title,
            body,
        } = r;
        let now = Instant::now();
        let e = self.st.entry(key).or_default();
        e.seen = true;
        if !over {
            e.since = None;
            e.fired = false;
            return;
        }
        let since = *e.since.get_or_insert(now);
        let cooled = e
            .last
            .is_none_or(|l| now.duration_since(l) >= Duration::from_secs(120));
        if !e.fired && cooled && now.duration_since(since) >= hold {
            e.fired = true;
            e.last = Some(now);
            self.log.push_back((ts, format!("{title}: {body}")));
            while self.log.len() > 20 {
                self.log.pop_front();
            }
            notify(title, &body, urgent);
        }
    }

    /// Chaves que sumiram (worktree sem processos) são rearmadas.
    fn end_round(&mut self) {
        for e in self.st.values_mut() {
            if !e.seen {
                e.since = None;
                e.fired = false;
            }
            e.seen = false;
        }
    }
}

fn notify(title: &str, body: &str, urgent: bool) {
    let (t, b) = (title.to_string(), body.to_string());
    thread::spawn(move || {
        let _ = Command::new("notify-send")
            .args([
                "-a",
                "wt-monitor",
                "-u",
                if urgent { "critical" } else { "normal" },
                "-i",
                "utilities-system-monitor",
                &t,
                &b,
            ])
            .status();
    });
}

fn sampler(root: PathBuf, out: Arc<Mutex<String>>, events: Arc<Mutex<VecDeque<Event>>>) {
    let cfg = Cfg::from_env();
    let mut alerts = Alerts::default();
    let mut cache = HashMap::new();
    let mut prev_t: Option<Instant> = None;
    let mut prev_pids: HashMap<u32, u64> = HashMap::new();
    let mut prev_cpus = BTreeMap::new();
    loop {
        let now = Instant::now();
        let dt = prev_t.map(|t| now.duration_since(t).as_secs_f64());
        let pids: Vec<u32> = fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .collect();
        let procs: Vec<Proc> = pids
            .into_iter()
            .filter_map(|p| proc_info(p, &root, &mut cache))
            .collect();
        let cpus = cpu_totals();

        let cores: Vec<f64> = cpus
            .iter()
            .map(|(c, &(tot, idle))| {
                let (pt, pi) = prev_cpus.get(c).copied().unwrap_or((tot, idle));
                let d = tot.saturating_sub(pt) as f64;
                if d > 0.0 {
                    100.0 * (1.0 - idle.saturating_sub(pi) as f64 / d)
                } else {
                    0.0
                }
            })
            .collect();

        let next_pids: HashMap<u32, u64> = procs.iter().map(|p| (p.pid, p.ticks)).collect();
        let mut groups: BTreeMap<PathBuf, Group> = BTreeMap::new();
        for p in procs {
            let pct = match (prev_pids.get(&p.pid), dt) {
                (Some(&pt), Some(dt)) if dt > 0.0 => {
                    100.0 * p.ticks.saturating_sub(pt) as f64 / TICK / dt
                }
                _ => 0.0,
            };
            let g = groups.entry(p.wt.clone()).or_insert_with(|| Group {
                cpu: 0.0,
                rss: 0,
                threads: 0,
                claude: false,
                nproc: 0,
                cores: BTreeSet::new(),
                aff: BTreeSet::new(),
                procs: vec![],
            });
            g.cpu += pct;
            g.rss += p.rss;
            g.threads += p.threads;
            g.nproc += 1;
            let first = p.cmd.split(' ').next().unwrap_or("");
            if first.rsplit('/').next() == Some("claude") {
                g.claude = true;
            }
            if pct > 1.0 {
                g.cores.insert(p.cpu);
            }
            g.aff.insert(p.aff.clone());
            g.procs.push((p, pct));
        }
        let mut groups: Vec<(PathBuf, Group)> = groups.into_iter().collect();
        groups.sort_by(|a, b| b.1.cpu.partial_cmp(&a.1.cpu).unwrap());

        let (mt, ma) = mem_info();
        let load = fs::read_to_string("/proc/loadavg").unwrap_or_default();
        let load: Vec<&str> = load.split_whitespace().take(3).collect();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let hold = Duration::from_secs(cfg.cpu_secs);
        for (path, g) in &groups {
            let name = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let key = path.display();
            alerts.check(
                ts,
                Rule {
                    key: format!("cpu:{key}"),
                    over: g.cpu >= cfg.cpu_pct,
                    hold,
                    urgent: false,
                    title: "CPU alta",
                    body: format!(
                        "{name}: {:.0}% (≈{:.1} cores) há {}s+",
                        g.cpu,
                        g.cpu / 100.0,
                        cfg.cpu_secs
                    ),
                },
            );
            alerts.check(
                ts,
                Rule {
                    key: format!("mem:{key}"),
                    over: g.rss >= cfg.mem_bytes,
                    hold: Duration::from_secs(5),
                    urgent: false,
                    title: "Memória alta",
                    body: format!("{name} usa {:.1} GB", g.rss as f64 / 1e9),
                },
            );
        }
        let used_pct = if mt > 0 {
            100.0 * mt.saturating_sub(ma) as f64 / mt as f64
        } else {
            0.0
        };
        alerts.check(
            ts,
            Rule {
                key: "sys".into(),
                over: used_pct >= cfg.sys_pct,
                hold: Duration::from_secs(10),
                urgent: true,
                title: "Memória do sistema",
                body: format!("{used_pct:.0}% em uso, {:.1} GB livres", ma as f64 / 1e9),
            },
        );
        alerts.end_round();

        let mut j = String::new();
        let _ = write!(
            j,
            "{{\"t\":{ts:.1},\"root\":\"{}\",\"mem\":{{\"total\":{mt},\"avail\":{ma}}},",
            esc(&root.to_string_lossy())
        );
        let _ = write!(j, "\"load\":[{}],", load.join(","));
        let _ = write!(
            j,
            "\"cfg\":{{\"cpu_pct\":{},\"cpu_secs\":{},\"mem_bytes\":{},\"sys_pct\":{}}},",
            cfg.cpu_pct, cfg.cpu_secs, cfg.mem_bytes, cfg.sys_pct
        );
        let _ = write!(
            j,
            "\"alerts\":[{}],",
            alerts
                .log
                .iter()
                .rev()
                .map(|(t, m)| format!("{{\"t\":{t:.0},\"msg\":\"{}\"}}", esc(m)))
                .collect::<Vec<_>>()
                .join(",")
        );
        let _ = write!(
            j,
            "\"events\":[{}],",
            events
                .lock()
                .unwrap()
                .iter()
                .rev()
                .map(|e| format!(
                "{{\"t\":{:.0},\"path\":\"{}\",\"name\":\"{}\",\"session\":\"{}\",\"msg\":\"{}\"}}",
                e.t, esc(&e.path), esc(&e.name), esc(&e.session), esc(&e.msg)
            ))
                .collect::<Vec<_>>()
                .join(",")
        );
        let _ = write!(
            j,
            "\"cores\":[{}],\"groups\":[",
            cores
                .iter()
                .map(|c| format!("{c:.1}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        for (gi, (path, g)) in groups.iter_mut().enumerate() {
            g.procs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let name = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            if gi > 0 {
                j.push(',');
            }
            let _ = write!(
                j,
                "{{\"path\":\"{}\",\"name\":\"{}\",\"cpu\":{:.1},\"rss\":{},\"threads\":{},\"nproc\":{},\"claude\":{},\"cores_in_use\":[{}],\"aff\":[{}],\"procs\":[",
                esc(&path.to_string_lossy()), esc(&name), g.cpu, g.rss, g.threads, g.nproc, g.claude,
                g.cores.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(","),
                g.aff.iter().map(|a| format!("\"{}\"", esc(a))).collect::<Vec<_>>().join(","),
            );
            for (pi, (p, pct)) in g.procs.iter().take(8).enumerate() {
                if pi > 0 {
                    j.push(',');
                }
                let _ = write!(
                    j,
                    "{{\"pid\":{},\"cmd\":\"{}\",\"pct\":{:.1},\"rss\":{},\"cpu\":{}}}",
                    p.pid,
                    esc(&p.cmd),
                    pct,
                    p.rss,
                    p.cpu
                );
            }
            j.push_str("]}");
        }
        j.push_str("]}");
        *out.lock().unwrap() = j;

        prev_t = Some(now);
        prev_pids = next_pids;
        prev_cpus = cpus;
        thread::sleep(Duration::from_secs(1));
    }
}

struct Event {
    t: f64,
    path: String,
    name: String,
    session: String,
    msg: String,
}

struct Ctx {
    data: Arc<Mutex<String>>,
    events: Arc<Mutex<VecDeque<Event>>>,
    root: PathBuf,
    port: u16,
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn respond(s: &mut TcpStream, status: &str, ct: &str, body: &str) {
    let _ = write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// O corpo das notificações aceita marcação; escapa para exibir a mensagem literalmente.
fn markup_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Corpo: linhas `cwd:` e `session:`, linha em branco, depois a mensagem do agente.
fn handle_event(ctx: &Ctx, body: &str) {
    let (head, msg) = body.split_once("\n\n").unwrap_or((body, ""));
    let field = |k: &str| {
        head.lines()
            .find_map(|l| l.strip_prefix(k))
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    let (cwd, session) = (field("cwd:"), field("session:"));
    let (path, name) = match worktree_of(Path::new(&cwd), &ctx.root, &mut HashMap::new()) {
        Some(p) => {
            let n = p
                .strip_prefix(&ctx.root)
                .unwrap_or(&p)
                .to_string_lossy()
                .into_owned();
            (p.to_string_lossy().into_owned(), n)
        }
        None => (
            cwd.clone(),
            cwd.rsplit('/').next().unwrap_or("agente").to_string(),
        ),
    };
    let msg: String = msg.trim().chars().take(600).collect();
    let preview: String = msg.chars().take(300).collect();
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    {
        let mut ev = ctx.events.lock().unwrap();
        ev.push_back(Event {
            t,
            path,
            name: name.clone(),
            session,
            msg,
        });
        while ev.len() > 30 {
            ev.pop_front();
        }
    }
    let body = if preview.is_empty() {
        "Tarefa concluída".to_string()
    } else {
        markup_escape(&preview)
    };
    notify(&format!("✔ {name} terminou"), &body, false);
}

/// O Claude Code nomeia a pasta do projeto trocando todo caractere não alfanumérico por '-'.
fn encode_project(p: &str) -> String {
    p.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Transcrição (.jsonl) mais recente do worktree, entre todas as pastas `~/.claude*`.
/// Prefere a pasta exata; só cai para subpastas se a sessão tiver sido aberta dentro delas.
fn latest_transcript(wt: &Path) -> Option<(PathBuf, SystemTime)> {
    let enc = encode_project(&wt.to_string_lossy());
    let home = std::env::var("HOME").ok()?;
    let mut dirs: Vec<(bool, PathBuf)> = vec![];
    for cfg in fs::read_dir(&home).ok()?.flatten() {
        let n = cfg.file_name().to_string_lossy().into_owned();
        if n != ".claude" && !n.starts_with(".claude-") {
            continue;
        }
        let Ok(rd) = fs::read_dir(cfg.path().join("projects")) else {
            continue;
        };
        for pd in rd.flatten() {
            let pn = pd.file_name().to_string_lossy().into_owned();
            if pn == enc {
                dirs.push((true, pd.path()));
            } else if pn.starts_with(&format!("{enc}-")) {
                dirs.push((false, pd.path()));
            }
        }
    }
    let pick = |exact: bool| {
        dirs.iter()
            .filter(|(e, _)| *e == exact)
            .flat_map(|(_, p)| fs::read_dir(p).into_iter().flatten().flatten())
            .filter(|f| f.path().extension().is_some_and(|x| x == "jsonl"))
            .filter_map(|f| Some((f.path(), f.metadata().ok()?.modified().ok()?)))
            .max_by_key(|(_, t)| *t)
    };
    pick(true).or_else(|| pick(false))
}

/// Últimas linhas completas do arquivo (descarta a primeira, possivelmente cortada, e linhas gigantes).
fn tail_lines(p: &Path, max_bytes: u64, max_line: usize, keep: usize) -> Vec<String> {
    let Ok(mut f) = File::open(p) else {
        return vec![];
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return vec![];
    }
    let mut b = Vec::new();
    let _ = f.read_to_end(&mut b);
    let text = String::from_utf8_lossy(&b);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let v: Vec<String> = lines
        .into_iter()
        .filter(|l| !l.is_empty() && l.len() <= max_line)
        .map(String::from)
        .collect();
    v[v.len().saturating_sub(keep)..].to_vec()
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let (mut o, mut i) = (Vec::new(), 0);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                o.push(v);
                i += 3;
                continue;
            }
        }
        o.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&o).into_owned()
}

/// Só serve transcrições de worktrees dentro da raiz monitorada; o caminho nunca vira leitura arbitrária.
fn preview(ctx: &Ctx, query: &str) -> String {
    let none = || "{\"found\":false}".to_string();
    let raw = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("path="))
        .map(pct_decode)
        .unwrap_or_default();
    let wt = PathBuf::from(&raw);
    let ok = wt.is_absolute()
        && wt.starts_with(&ctx.root)
        && !wt.components().any(|c| matches!(c, Component::ParentDir))
        && wt.join(".git").exists();
    if !ok {
        return none();
    }
    let Some((file, mtime)) = latest_transcript(&wt) else {
        return none();
    };
    let lines = tail_lines(&file, 400_000, 60_000, 60);
    let session = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mt = mtime
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    format!(
        "{{\"found\":true,\"session\":\"{}\",\"mtime\":{mt:.0},\"lines\":[{}]}}",
        esc(&session),
        lines
            .iter()
            .map(|l| format!("\"{}\"", esc(l)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn serve(mut s: TcpStream, ctx: &Ctx) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = find(&buf, b"\r\n\r\n") {
            break i;
        }
        if buf.len() > 65536 {
            return;
        }
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut rl = head.lines().next().unwrap_or("").split_whitespace();
    let (method, path) = (rl.next().unwrap_or(""), rl.next().unwrap_or("/"));
    let hdr = |k: &str| {
        head.lines().skip(1).find_map(|l| {
            let (n, v) = l.split_once(':')?;
            n.trim()
                .eq_ignore_ascii_case(k)
                .then(|| v.trim().to_string())
        })
    };

    // Bloqueia DNS rebinding: só aceita o Host que o próprio painel usa.
    let host = hdr("host").unwrap_or_default();
    if host != format!("127.0.0.1:{}", ctx.port) && host != format!("localhost:{}", ctx.port) {
        return respond(&mut s, "403 Forbidden", "text/plain", "forbidden host");
    }
    let (route, query) = path.split_once('?').unwrap_or((path, ""));

    if method == "POST" && path == "/event" {
        // Cabeçalho customizado força preflight CORS: páginas web de terceiros não conseguem postar aqui.
        if hdr("x-wt").as_deref() != Some("1") {
            return respond(&mut s, "403 Forbidden", "text/plain", "forbidden");
        }
        let len: usize = hdr("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
            .min(65536);
        let mut body = buf[head_end + 4..].to_vec();
        while body.len() < len {
            match s.read(&mut tmp) {
                Ok(0) | Err(_) => break,
                Ok(n) => body.extend_from_slice(&tmp[..n]),
            }
        }
        handle_event(ctx, &String::from_utf8_lossy(&body[..body.len().min(len)]));
        return respond(&mut s, "204 No Content", "text/plain", "");
    }

    if route == "/api/preview" {
        return respond(&mut s, "200 OK", "application/json", &preview(ctx, query));
    }
    if route == "/api" {
        let json = ctx.data.lock().unwrap().clone();
        respond(&mut s, "200 OK", "application/json", &json);
    } else {
        respond(&mut s, "200 OK", "text/html; charset=utf-8", INDEX);
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .map(|r| {
            if let Some(rest) = r.strip_prefix("~/") {
                format!("{}/{rest}", std::env::var("HOME").unwrap_or_default())
            } else {
                r
            }
        })
        .unwrap_or_else(|| format!("{}/dev", std::env::var("HOME").unwrap_or_default()));
    let root = fs::canonicalize(&root).unwrap_or_else(|e| {
        eprintln!("raiz inválida {root}: {e}");
        std::process::exit(1);
    });
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(8765);

    let ctx = Arc::new(Ctx {
        data: Arc::new(Mutex::new(String::from("{}"))),
        events: Arc::new(Mutex::new(VecDeque::new())),
        root: root.clone(),
        port,
    });
    {
        let c = ctx.clone();
        thread::spawn(move || sampler(c.root.clone(), c.data.clone(), c.events.clone()));
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| {
        eprintln!("não foi possível abrir a porta {port}: {e}");
        std::process::exit(1);
    });
    println!(
        "Painel em http://127.0.0.1:{port}  (raiz: {})",
        root.display()
    );
    for s in listener.incoming().flatten() {
        let c = ctx.clone();
        thread::spawn(move || serve(s, &c));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_escape() {
        assert_eq!(esc("a\"b\\c\nd"), "a\\\"b\\\\c\\u000ad");
    }

    #[test]
    fn project_dir_encoding_matches_claude_code() {
        assert_eq!(
            encode_project("/home/u/dev/app/.worktrees/x"),
            "-home-u-dev-app--worktrees-x"
        );
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(pct_decode("%2Fhome%2Fu%20dev"), "/home/u dev");
        assert_eq!(pct_decode("100%"), "100%");
    }

    #[test]
    fn notification_markup_is_escaped() {
        assert_eq!(
            markup_escape("<b>a</b> & c"),
            "&lt;b&gt;a&lt;/b&gt; &amp; c"
        );
    }

    #[test]
    fn worktree_is_nearest_dir_with_dot_git() {
        let base = std::env::temp_dir().join(format!("wt-monitor-test-{}", std::process::id()));
        let wt = base.join("repo/.worktrees/feat");
        fs::create_dir_all(wt.join("src")).unwrap();
        fs::write(wt.join(".git"), "gitdir: x").unwrap();
        let found = worktree_of(&wt.join("src"), &base, &mut HashMap::new());
        assert_eq!(found, Some(wt));
        assert_eq!(worktree_of(&base, &base, &mut HashMap::new()), None);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tail_keeps_last_complete_lines() {
        let f = std::env::temp_dir().join(format!("wt-monitor-tail-{}.jsonl", std::process::id()));
        fs::write(&f, "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n").unwrap();
        assert_eq!(tail_lines(&f, 1000, 100, 2), vec!["{\"a\":2}", "{\"a\":3}"]);
        let _ = fs::remove_file(&f);
    }
}
