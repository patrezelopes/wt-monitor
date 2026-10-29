//! Contenção de CPU (PSI e tempo na fila de execução) e prioridade por worktree via cgroup v2.
//! A medição é só leitura e está sempre ligada; o controle é opt-in (`--control`). Veja docs/architecture.md.

use crate::{esc, pct_decode, qparam, respond, terms, Ctx};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const CG_ROOT: &str = "/sys/fs/cgroup";
/// Prefixo dos cgroups criados pelo painel; tudo o que começa assim é nosso.
const PREFIX: &str = "wtm-";
const NEUTRAL_WEIGHT: u32 = 100;
pub const FOCUS_WEIGHT: u32 = 1000;
pub const BACKGROUND_WEIGHT: u32 = 20;
const GIB: u64 = 1 << 30;
const MAX_GIB: u64 = 1024;
/// Abaixo disto de tempo pronto-para-rodar por amostra, o percentual de espera não significa nada.
const MIN_READY_NS: u64 = 20_000_000;

// ---------- medição ----------

/// Média de 10 s da linha `some` ou `full` de um arquivo PSI (`/proc/pressure/*`).
pub fn psi_avg10(text: &str, kind: &str) -> Option<f64> {
    text.lines().find_map(|l| {
        let rest = l.strip_prefix(kind)?.trim_start();
        rest.split_whitespace()
            .find_map(|f| f.strip_prefix("avg10=")?.parse().ok())
    })
}

/// Pressão do sistema (percentual do tempo em que alguma tarefa esperou). `None` sem suporte no kernel.
pub fn psi_json() -> String {
    let read = |f: &str, kind: &str| {
        fs::read_to_string(format!("/proc/pressure/{f}"))
            .ok()
            .and_then(|t| psi_avg10(&t, kind))
    };
    let num = |v: Option<f64>| v.map_or("null".to_string(), |x| format!("{x:.2}"));
    format!(
        "{{\"cpu\":{},\"mem\":{},\"memfull\":{}}}",
        num(read("cpu", "some")),
        num(read("memory", "some")),
        num(read("memory", "full"))
    )
}

/// `/proc/<pid>/schedstat`: nanossegundos executando e esperando na fila.
pub fn parse_schedstat(s: &str) -> Option<(u64, u64)> {
    let mut it = s.split_whitespace();
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// (executando, esperando) em ns, somando as threads do processo (o arquivo do pid só conta a thread principal).
pub fn proc_sched(pid: u32) -> Option<(u64, u64)> {
    let tasks = fs::read_dir(format!("/proc/{pid}/task")).ok()?;
    let (mut run, mut wait) = (0u64, 0u64);
    for t in tasks.flatten().take(256) {
        if let Some((r, w)) = fs::read_to_string(t.path().join("schedstat"))
            .ok()
            .and_then(|s| parse_schedstat(&s))
        {
            run += r;
            wait += w;
        }
    }
    Some((run, wait))
}

pub struct Wait {
    /// Parte do tempo pronto-para-rodar que foi gasta esperando por um core.
    pub pct: f64,
    /// Quantos cores de trabalho ficaram esperando, em média.
    pub waiting: f64,
    /// Quantos cores de trabalho estavam prontos (rodando + esperando), em média.
    pub ready: f64,
}

pub fn wait_stats(d_run: u64, d_wait: u64, dt: f64) -> Wait {
    let ready_ns = d_run + d_wait;
    let per = |ns: u64| if dt > 0.0 { ns as f64 / 1e9 / dt } else { 0.0 };
    Wait {
        pct: if ready_ns >= MIN_READY_NS {
            100.0 * d_wait as f64 / ready_ns as f64
        } else {
            0.0
        },
        waiting: per(d_wait),
        ready: per(ready_ns),
    }
}

// ---------- controle ----------

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Level {
    Low,
    #[default]
    Normal,
    High,
}

impl Level {
    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "baixa" => Some(Level::Low),
            "normal" => Some(Level::Normal),
            "alta" => Some(Level::High),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Level::Low => "baixa",
            Level::Normal => "normal",
            Level::High => "alta",
        }
    }
    fn weight(self) -> u32 {
        match self {
            Level::Low => 25,
            Level::Normal => NEUTRAL_WEIGHT,
            Level::High => 400,
        }
    }
}

/// O que o usuário pediu para um worktree. `high`/`low` em bytes; 0 = sem limite/proteção.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct Want {
    cpu: Level,
    high: u64,
    low: u64,
}

/// O que o painel aplicou de fato num worktree (estado do cgroup).
struct Applied {
    dir: PathBuf,
    /// Cgroup de onde cada processo veio, para devolvê-lo ao sair da prioridade.
    origins: HashMap<u32, String>,
    written: Option<(u32, u64, u64)>,
    inside: usize,
    err: String,
    mem_cur: u64,
    high_events: u64,
}

struct Inner {
    base: PathBuf,
    cg_root: PathBuf,
    want: HashMap<PathBuf, Want>,
    focus: Option<PathBuf>,
    applied: HashMap<PathBuf, Applied>,
}

pub struct Control {
    pub enabled: bool,
    reason: String,
    mem_low: bool,
    inner: Option<Mutex<Inner>>,
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `0::/caminho` de `/proc/<pid>/cgroup` (cgroup v2).
fn origin_of(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/cgroup"))
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("0::").map(str::to_string))
}

fn read_u64(p: &Path) -> u64 {
    fs::read_to_string(p)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Valor da linha `chave N` de um arquivo como `memory.events`.
fn flat_key(p: &Path, key: &str) -> u64 {
    fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix(key)?.trim().parse().ok())
        .unwrap_or(0)
}

/// O cgroup `user@UID.service` do usuário: é onde o systemd delega os controladores cpu e memory.
fn detect_base(cg_root: &Path) -> Result<PathBuf, String> {
    let own = fs::read_to_string("/proc/self/cgroup").map_err(|e| e.to_string())?;
    let rel = own
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .ok_or_else(|| "cgroup v2 indisponível neste sistema".to_string())?;
    let mut dir = cg_root.to_path_buf();
    for comp in rel.split('/').filter(|c| !c.is_empty()) {
        dir.push(comp);
        if comp.starts_with("user@") && comp.ends_with(".service") {
            return probe(dir);
        }
    }
    Err("não há um systemd --user com cgroups delegados (é preciso systemd e cgroup v2)".into())
}

/// Cria e remove um cgroup de teste para ter certeza de que dá para escrever e de que cpu/memory estão delegados.
fn probe(dir: PathBuf) -> Result<PathBuf, String> {
    let p = dir.join(format!("{PREFIX}probe-{}", std::process::id()));
    fs::create_dir(&p)
        .map_err(|e| format!("sem permissão para criar cgroups em {}: {e}", dir.display()))?;
    let ok = ["cpu.weight", "memory.high", "memory.low"]
        .iter()
        .all(|f| p.join(f).exists());
    let _ = fs::remove_dir(&p);
    if ok {
        Ok(dir)
    } else {
        Err("os controladores cpu e memory não estão delegados ao seu usuário".into())
    }
}

fn reset_files(dir: &Path) {
    let _ = fs::write(dir.join("cpu.weight"), NEUTRAL_WEIGHT.to_string());
    let _ = fs::write(dir.join("memory.high"), "max");
    let _ = fs::write(dir.join("memory.low"), "0");
}

impl Control {
    pub fn new(enabled: bool) -> Self {
        if !enabled {
            return Self::unsupported(false, String::new());
        }
        match detect_base(Path::new(CG_ROOT)) {
            Ok(base) => {
                let c = Self::with_base(base, PathBuf::from(CG_ROOT));
                c.release_leftovers();
                c
            }
            Err(reason) => Self::unsupported(true, reason),
        }
    }

    fn unsupported(enabled: bool, reason: String) -> Self {
        Control {
            enabled,
            reason,
            mem_low: false,
            inner: None,
        }
    }

    fn with_base(base: PathBuf, cg_root: PathBuf) -> Self {
        // Sem `memory_recursiveprot`, o kernel ignora o memory.low de cgroups filhos do usuário.
        let mem_low = fs::read_to_string("/proc/mounts")
            .unwrap_or_default()
            .lines()
            .any(|l| l.contains(" cgroup2 ") && l.contains("memory_recursiveprot"));
        Control {
            enabled: true,
            reason: String::new(),
            mem_low,
            inner: Some(Mutex::new(Inner {
                base,
                cg_root,
                want: HashMap::new(),
                focus: None,
                applied: HashMap::new(),
            })),
        }
    }

    pub fn supported(&self) -> bool {
        self.inner.is_some()
    }

    /// Ao iniciar, o painel não lembra o que fez antes: zera os pesos dos grupos que sobraram.
    /// Os que ainda têm processos ficam (com pesos neutros) e são reaproveitados depois.
    fn release_leftovers(&self) {
        let Some(inner) = &self.inner else { return };
        let inner = inner.lock().unwrap();
        for e in fs::read_dir(&inner.base).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().starts_with(PREFIX) && e.path().is_dir() {
                reset_files(&e.path());
                let _ = fs::remove_dir(e.path());
            }
        }
    }

    /// Atualiza o pedido de um worktree (campo ausente = não muda). O sampler aplica no próximo ciclo.
    pub fn set(&self, wt: &Path, cpu: Option<Level>, high: Option<u64>, low: Option<u64>) {
        let Some(inner) = &self.inner else { return };
        let mut st = inner.lock().unwrap();
        let mut w = st.want.get(wt).copied().unwrap_or_default();
        if let Some(c) = cpu {
            w.cpu = c;
        }
        if let Some(h) = high {
            w.high = h;
        }
        if let Some(l) = low {
            w.low = l;
        }
        if w == Want::default() {
            st.want.remove(wt);
        } else {
            st.want.insert(wt.to_path_buf(), w);
        }
    }

    /// Liga o Foco num worktree (todos os outros vão para segundo plano) ou desliga com `None`.
    pub fn set_focus(&self, wt: Option<PathBuf>) {
        if let Some(inner) = &self.inner {
            inner.lock().unwrap().focus = wt;
        }
    }

    /// Chamado pelo sampler a cada ciclo com os processos de cada worktree: aplica o que foi pedido.
    pub fn enforce(&self, groups: &[(PathBuf, Vec<u32>)]) {
        let Some(inner) = &self.inner else { return };
        let mut st = inner.lock().unwrap();
        let me = std::process::id();
        let mut seen = HashSet::new();
        for (wt, pids) in groups {
            seen.insert(wt.clone());
            let (weight, want) = st.effective(wt);
            if needs_cgroup(weight, want) {
                st.apply(wt, weight, want, pids, me, true);
            } else if st.applied.contains_key(wt) {
                st.release(wt);
            }
        }
        // worktrees que ficaram sem processos e já não pedem nada
        let idle: Vec<PathBuf> = st
            .applied
            .keys()
            .filter(|w| !seen.contains(*w))
            .filter(|w| {
                let (weight, want) = st.effective(w);
                !needs_cgroup(weight, want)
            })
            .cloned()
            .collect();
        for w in idle {
            st.release(&w);
        }
    }

    /// Um processo novo do painel (terminal) já nasce no grupo do worktree, antes de alocar memória.
    pub fn on_spawn(&self, wt: &Path, pid: u32) {
        let Some(inner) = &self.inner else { return };
        let mut st = inner.lock().unwrap();
        let (weight, want) = st.effective(wt);
        if needs_cgroup(weight, want) {
            st.apply(wt, weight, want, &[pid], std::process::id(), false);
        }
    }

    /// Estado de controle de um worktree, para o JSON do painel.
    pub fn group_json(&self, wt: &Path) -> String {
        let Some(inner) = &self.inner else {
            return "null".into();
        };
        let st = inner.lock().unwrap();
        let (weight, want) = st.effective(wt);
        let label = match &st.focus {
            Some(f) if f == wt => "foco",
            Some(_) => "fundo",
            None => want.cpu.name(),
        };
        let (inside, err, cur, hev) = st.applied.get(wt).map_or((0, "", 0, 0), |a| {
            (a.inside, &a.err, a.mem_cur, a.high_events)
        });
        format!(
            "{{\"cpu\":\"{label}\",\"want\":\"{}\",\"weight\":{weight},\"high\":{},\"low\":{},\"inside\":{inside},\"mem\":{cur},\"throttled\":{hev},\"err\":\"{}\"}}",
            want.cpu.name(),
            want.high,
            want.low,
            esc(err)
        )
    }

    /// Estado global do controle, para o JSON do painel.
    pub fn head_json(&self) -> String {
        let focus = self
            .inner
            .as_ref()
            .and_then(|i| i.lock().unwrap().focus.clone())
            .map_or("null".to_string(), |f| {
                format!("\"{}\"", esc(&f.to_string_lossy()))
            });
        format!(
            "{{\"enabled\":{},\"supported\":{},\"reason\":\"{}\",\"mem_low\":{},\"focus\":{focus}}}",
            self.enabled,
            self.supported(),
            esc(&self.reason),
            self.mem_low
        )
    }
}

fn needs_cgroup(weight: u32, want: Want) -> bool {
    weight != NEUTRAL_WEIGHT || want.high > 0 || want.low > 0
}

impl Inner {
    /// Peso de CPU efetivo (o Foco sobrepõe o nível pedido) e o pedido de cada worktree.
    fn effective(&self, wt: &Path) -> (u32, Want) {
        let want = self.want.get(wt).copied().unwrap_or_default();
        let weight = match &self.focus {
            Some(f) if f == wt => FOCUS_WEIGHT,
            Some(_) => BACKGROUND_WEIGHT,
            None => want.cpu.weight(),
        };
        (weight, want)
    }

    fn dir_of(&self, wt: &Path) -> PathBuf {
        self.base
            .join(format!("{PREFIX}{:016x}", fnv(&wt.to_string_lossy())))
    }

    /// Caminho do cgroup como aparece em `/proc/<pid>/cgroup`.
    fn cg_path(&self, dir: &Path) -> String {
        format!(
            "/{}",
            dir.strip_prefix(&self.cg_root)
                .unwrap_or(dir)
                .to_string_lossy()
        )
    }

    fn apply(&mut self, wt: &Path, weight: u32, want: Want, pids: &[u32], me: u32, prune: bool) {
        let dir = self.dir_of(wt);
        let target = self.cg_path(&dir);
        let a = self.applied.entry(wt.to_path_buf()).or_insert(Applied {
            dir: dir.clone(),
            origins: HashMap::new(),
            written: None,
            inside: 0,
            err: String::new(),
            mem_cur: 0,
            high_events: 0,
        });
        if !a.dir.exists() {
            if let Err(e) = fs::create_dir(&a.dir) {
                a.err = format!("não foi possível criar o cgroup: {e}");
                return;
            }
            a.written = None;
        }
        let values = (weight, want.high, want.low);
        if a.written != Some(values) {
            let num = |v: u64, none: &str| {
                if v == 0 {
                    none.to_string()
                } else {
                    v.to_string()
                }
            };
            let errs: Vec<String> = [
                ("cpu.weight", weight.to_string()),
                ("memory.high", num(want.high, "max")),
                ("memory.low", num(want.low, "0")),
            ]
            .into_iter()
            .filter_map(|(f, v)| {
                fs::write(a.dir.join(f), v)
                    .err()
                    .map(|e| format!("{f}: {e}"))
            })
            .collect();
            a.err = errs.join("; ");
            a.written = Some(values);
        }
        if prune {
            let live: HashSet<u32> = pids.iter().copied().collect();
            a.origins.retain(|p, _| live.contains(p));
            a.inside = 0;
        }
        for &pid in pids {
            if pid == me {
                continue; // o painel nunca se rebaixa
            }
            let Some(cur) = origin_of(pid) else { continue };
            if cur == target {
                a.inside += 1;
            } else if fs::write(a.dir.join("cgroup.procs"), pid.to_string()).is_ok() {
                a.origins.entry(pid).or_insert(cur);
                a.inside += 1;
            }
        }
        a.mem_cur = read_u64(&a.dir.join("memory.current"));
        a.high_events = flat_key(&a.dir.join("memory.events"), "high");
    }

    /// Devolve os processos ao cgroup de origem, zera os pesos e remove o grupo.
    fn release(&mut self, wt: &Path) {
        let Some(a) = self.applied.remove(wt) else {
            return;
        };
        let inside = fs::read_to_string(a.dir.join("cgroup.procs")).unwrap_or_default();
        for pid in inside.lines().filter_map(|l| l.trim().parse::<u32>().ok()) {
            if let Some(origin) = a.origins.get(&pid) {
                let dest = self.cg_root.join(origin.trim_start_matches('/'));
                let _ = fs::write(dest.join("cgroup.procs"), pid.to_string());
            }
        }
        reset_files(&a.dir);
        // Falha se sobraram processos de origem desconhecida: o grupo segue, com pesos neutros.
        let _ = fs::remove_dir(&a.dir);
    }
}

// ---------- API HTTP ----------

fn bad(s: &mut TcpStream, msg: &str) {
    respond(s, "400 Bad Request", "text/plain", msg);
}

/// Limite em GiB (inteiro, 0 = sem limite) → bytes.
fn gib(v: &str) -> Option<u64> {
    v.parse::<u64>()
        .ok()
        .filter(|g| *g <= MAX_GIB)
        .map(|g| g * GIB)
}

/// Rotas `/api/control*`. Mudar prioridade exige o mesmo token dos terminais.
pub fn route(
    mut s: TcpStream,
    ctx: &Ctx,
    method: &str,
    route: &str,
    hdr: &dyn Fn(&str) -> Option<String>,
    body: &str,
) {
    let ctl = &ctx.control;
    if !ctl.enabled {
        return respond(
            &mut s,
            "404 Not Found",
            "text/plain",
            "control is disabled (start with --control)",
        );
    }
    let authed = terms::authed(&ctx.token, hdr);
    if method == "GET" && route == "/api/control" {
        let json = format!(
            "{{\"enabled\":true,\"auth\":{authed},\"supported\":{}}}",
            ctl.supported()
        );
        return respond(&mut s, "200 OK", "application/json", &json);
    }
    if !authed {
        return respond(&mut s, "403 Forbidden", "text/plain", "forbidden");
    }
    if !ctl.supported() {
        return respond(&mut s, "503 Service Unavailable", "text/plain", &ctl.reason);
    }
    let worktree = |s: &mut TcpStream| -> Option<PathBuf> {
        let raw = qparam(body, "path").map(pct_decode).unwrap_or_default();
        let wt = ctx.worktree(&raw);
        if wt.is_none() {
            bad(s, "path is not a worktree inside the watched root");
        }
        wt
    };
    match (method, route) {
        ("POST", "/api/control") => {
            let Some(wt) = worktree(&mut s) else { return };
            let cpu = match qparam(body, "cpu") {
                None => None,
                Some(v) => match Level::parse(v) {
                    Some(l) => Some(l),
                    None => return bad(&mut s, "cpu must be baixa, normal or alta"),
                },
            };
            let mem = |key: &str| match qparam(body, key) {
                None => Ok(None),
                Some(v) => gib(v).map(Some).ok_or(()),
            };
            let (Ok(high), Ok(low)) = (mem("high"), mem("low")) else {
                return bad(&mut s, "high and low are whole GiB, 0 to 1024");
            };
            if cpu.is_none() && high.is_none() && low.is_none() {
                return bad(&mut s, "nothing to change (cpu, high or low)");
            }
            ctl.set(&wt, cpu, high, low);
            respond(&mut s, "204 No Content", "text/plain", "")
        }
        ("POST", "/api/control/focus") => {
            let empty = qparam(body, "path").is_none_or(str::is_empty);
            let wt = if empty {
                None
            } else {
                let Some(wt) = worktree(&mut s) else { return };
                Some(wt)
            };
            ctl.set_focus(wt);
            respond(&mut s, "204 No Content", "text/plain", "")
        }
        _ => respond(&mut s, "404 Not Found", "text/plain", "not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    #[test]
    fn psi_lines_are_parsed() {
        let t = "some avg10=14.25 avg60=9.00 avg300=3.10 total=123\nfull avg10=0.50 avg60=0.10 avg300=0.00 total=9\n";
        assert_eq!(psi_avg10(t, "some"), Some(14.25));
        assert_eq!(psi_avg10(t, "full"), Some(0.5));
        assert_eq!(psi_avg10("", "some"), None);
        assert_eq!(psi_avg10("some avg60=1.0", "some"), None);
    }

    #[test]
    fn schedstat_is_parsed() {
        assert_eq!(parse_schedstat("1500 700 12\n"), Some((1500, 700)));
        assert_eq!(parse_schedstat("x y"), None);
        assert_eq!(parse_schedstat(""), None);
    }

    #[test]
    fn wait_share_ignores_noise() {
        // 0,6 s rodando e 0,4 s esperando numa janela de 1 s
        let w = wait_stats(600_000_000, 400_000_000, 1.0);
        assert!((w.pct - 40.0).abs() < 1e-9 && (w.waiting - 0.4).abs() < 1e-9);
        assert!((w.ready - 1.0).abs() < 1e-9);
        // quase nada pronto para rodar: não vale alarmar
        assert_eq!(wait_stats(1_000_000, 5_000_000, 1.0).pct, 0.0);
        assert_eq!(wait_stats(0, 0, 0.0).waiting, 0.0);
    }

    #[test]
    fn levels_and_limits_are_validated() {
        assert_eq!(Level::parse("alta"), Some(Level::High));
        assert_eq!(Level::parse("baixa"), Some(Level::Low));
        assert_eq!(Level::parse("kill"), None);
        assert!(Level::Low.weight() < Level::Normal.weight());
        assert!(Level::Normal.weight() < Level::High.weight());
        assert_eq!(gib("4"), Some(4 * GIB));
        assert_eq!(gib("0"), Some(0));
        assert_eq!(gib("1025"), None);
        assert_eq!(gib("-1"), None);
        assert_eq!(gib("4.5"), None);
    }

    #[test]
    fn group_names_are_stable_per_path() {
        assert_eq!(fnv("/a/b"), fnv("/a/b"));
        assert_ne!(fnv("/a/b"), fnv("/a/c"));
    }

    fn sleeper() -> Child {
        Command::new("sleep")
            .arg("30")
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn read(p: PathBuf) -> String {
        fs::read_to_string(p).unwrap_or_default().trim().to_string()
    }

    /// Roda o ciclo completo contra diretórios comuns no lugar do cgroupfs.
    #[test]
    fn applies_and_releases_priority() {
        let tmp = std::env::temp_dir().join(format!("wt-monitor-sched-{}", std::process::id()));
        let (root, base) = (tmp.join("cg"), tmp.join("cg/user.slice/user@1.service"));
        fs::create_dir_all(&base).unwrap();
        let mut child = sleeper();
        let pid = child.id();
        let origin = origin_of(pid).unwrap();
        fs::create_dir_all(root.join(origin.trim_start_matches('/'))).unwrap();

        let c = Control::with_base(base.clone(), root.clone());
        let wt = PathBuf::from("/proj/a");
        let dir = base.join(format!("{PREFIX}{:016x}", fnv("/proj/a")));
        let groups = vec![(wt.clone(), vec![pid, std::process::id()])];

        c.enforce(&groups);
        assert!(!dir.exists(), "sem pedido, nada é criado");

        c.set(&wt, Some(Level::High), Some(2 * GIB), Some(GIB));
        c.enforce(&groups);
        assert_eq!(read(dir.join("cpu.weight")), "400");
        assert_eq!(read(dir.join("memory.high")), (2 * GIB).to_string());
        assert_eq!(read(dir.join("memory.low")), GIB.to_string());
        assert_eq!(read(dir.join("cgroup.procs")), pid.to_string());
        let j = c.group_json(&wt);
        assert!(
            j.contains("\"cpu\":\"alta\"") && j.contains("\"inside\":1"),
            "{j}"
        );

        // o Foco em outro worktree rebaixa este; desligar volta ao pedido original
        c.set_focus(Some(PathBuf::from("/proj/b")));
        c.enforce(&groups);
        assert_eq!(read(dir.join("cpu.weight")), BACKGROUND_WEIGHT.to_string());
        assert!(c.group_json(&wt).contains("\"cpu\":\"fundo\""));
        c.set_focus(None);
        c.enforce(&groups);
        assert_eq!(read(dir.join("cpu.weight")), "400");

        // voltar ao normal zera os pesos e devolve o processo à origem
        c.set(&wt, Some(Level::Normal), Some(0), Some(0));
        c.enforce(&groups);
        assert_eq!(read(dir.join("cpu.weight")), "100");
        assert_eq!(read(dir.join("memory.high")), "max");
        assert_eq!(
            read(
                root.join(origin.trim_start_matches('/'))
                    .join("cgroup.procs")
            ),
            pid.to_string()
        );
        assert!(c.group_json(&wt).contains("\"cpu\":\"normal\""));

        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn disabled_or_unsupported_control_does_nothing() {
        let off = Control::new(false);
        assert!(!off.enabled && !off.supported());
        off.set(Path::new("/x"), Some(Level::Low), None, None);
        off.enforce(&[(PathBuf::from("/x"), vec![1])]);
        assert_eq!(off.group_json(Path::new("/x")), "null");
        assert!(off.head_json().contains("\"enabled\":false"));
    }
}
