//! Terminais iniciados pelo próprio painel (opt-in com `--terminals`): PTY + WebSocket.
//! O painel é dono de cada processo; nada se conecta a terminais de fora. Veja docs/architecture.md.

use crate::{esc, pct_decode, pty, qparam, respond, ws, Ctx};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_TERMS: usize = 16;
/// Saída guardada por terminal para quem se conecta (ou reconecta) depois.
const RING_BYTES: usize = 256 * 1024;
/// Terminais encerrados continuam listados (com a última saída) por este tempo.
const DEAD_TTL: Duration = Duration::from_secs(600);

pub enum Msg {
    Data(Vec<u8>),
    Exit(i32),
}

pub struct Term {
    pub id: u32,
    pub path: String,
    pub name: String,
    pub kind: &'static str,
    created: f64,
    pid: u32,
    master: Mutex<File>,
    alive: AtomicBool,
    exit: Mutex<Option<i32>>,
    died: Mutex<Option<Instant>>,
    ring: Mutex<Vec<u8>>,
    subs: Mutex<Vec<Sender<Msg>>>,
}

impl Term {
    /// Ordem dos travamentos, sempre: ring → subs → exit.
    fn push(&self, chunk: &[u8]) {
        let mut ring = self.ring.lock().unwrap();
        ring.extend_from_slice(chunk);
        if ring.len() > RING_BYTES {
            let cut = ring.len() - RING_BYTES;
            ring.drain(..cut);
        }
        self.subs
            .lock()
            .unwrap()
            .retain(|s| s.send(Msg::Data(chunk.to_vec())).is_ok());
    }

    fn finish(&self, code: i32) {
        let mut subs = self.subs.lock().unwrap();
        *self.exit.lock().unwrap() = Some(code);
        *self.died.lock().unwrap() = Some(Instant::now());
        self.alive.store(false, Ordering::SeqCst);
        for s in subs.drain(..) {
            let _ = s.send(Msg::Exit(code));
        }
    }

    /// Saída acumulada + canal para o que vier depois (sem lacunas entre os dois).
    fn attach(&self) -> (Vec<u8>, Receiver<Msg>) {
        let ring = self.ring.lock().unwrap();
        let mut subs = self.subs.lock().unwrap();
        let (tx, rx) = channel();
        match *self.exit.lock().unwrap() {
            Some(code) => {
                let _ = tx.send(Msg::Exit(code));
            }
            None => subs.push(tx),
        }
        (ring.clone(), rx)
    }

    fn write_input(&self, data: &[u8]) -> std::io::Result<()> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("terminal encerrado"));
        }
        self.master.lock().unwrap().write_all(data)
    }

    fn resize(&self, cols: u16, rows: u16) {
        let _ = pty::resize(
            &self.master.lock().unwrap(),
            cols.clamp(1, 500),
            rows.clamp(1, 300),
        );
    }

    fn kill(self: &Arc<Self>) {
        pty::signal_group(self.pid, pty::SIGHUP);
        let t = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(2));
            if t.alive.load(Ordering::SeqCst) {
                pty::signal_group(t.pid, pty::SIGKILL);
            }
        });
    }
}

pub struct Terms {
    pub enabled: bool,
    token: String,
    items: Mutex<HashMap<u32, Arc<Term>>>,
    next: AtomicU32,
}

fn random_token() -> std::io::Result<String> {
    let mut b = [0u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// `$WT_TOKEN_FILE`, senão `$XDG_CONFIG_HOME/wt-monitor/token`, senão `~/.config/wt-monitor/token`.
fn token_path() -> PathBuf {
    if let Ok(p) = std::env::var("WT_TOKEN_FILE") {
        return PathBuf::from(p);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config")
        });
    base.join("wt-monitor").join("token")
}

/// O segredo que autoriza abrir terminais. Fica num arquivo só do usuário (0600) e nunca é devolvido
/// pela API: quem abre o painel o recebe uma vez, no fragmento do link (`#token=…`).
pub fn load_or_create_token() -> std::io::Result<String> {
    let path = token_path();
    if let Ok(t) = fs::read_to_string(&path) {
        let t = t.trim();
        if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            return Ok(t.to_string());
        }
    }
    if let Some(dir) = path.parent() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let token = random_token()?;
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    f.write_all(token.as_bytes())?;
    Ok(token)
}

fn ct_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// O pedido traz o cabeçalho customizado (que força preflight CORS, nunca aprovado) e o token certo.
pub fn authed(token: &str, hdr: &dyn Fn(&str) -> Option<String>) -> bool {
    hdr("x-wt").as_deref() == Some("1") && hdr("x-wt-token").is_some_and(|t| ct_eq(&t, token))
}

impl Terms {
    pub fn new(enabled: bool, token: String) -> Self {
        Terms {
            enabled,
            token,
            items: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
        }
    }

    fn sweep(&self, items: &mut HashMap<u32, Arc<Term>>) {
        items.retain(|_, t| {
            t.alive.load(Ordering::SeqCst)
                || t.died
                    .lock()
                    .unwrap()
                    .is_none_or(|d| d.elapsed() < DEAD_TTL)
        });
    }

    fn get(&self, id: u32) -> Option<Arc<Term>> {
        self.items.lock().unwrap().get(&id).cloned()
    }

    fn remove(&self, id: u32) -> bool {
        let t = self.items.lock().unwrap().remove(&id);
        t.inspect(|t| t.kill()).is_some()
    }

    /// `on_spawn` recebe o pid logo após o fork, antes de qualquer comando ser digitado.
    fn create(
        &self,
        wt: &Path,
        name: &str,
        kind: &'static str,
        size: (u16, u16),
        on_spawn: &dyn Fn(u32),
    ) -> Result<Arc<Term>, String> {
        let (cols, rows) = size;
        let mut items = self.items.lock().unwrap();
        self.sweep(&mut items);
        if items.len() >= MAX_TERMS {
            return Err(format!("limite de {MAX_TERMS} terminais atingido"));
        }
        // login interativo: carrega o perfil do usuário (PATH, nvm, troca de conta do Claude por pasta…)
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| Path::new(s).is_absolute() && Path::new(s).is_file())
            .unwrap_or_else(|| "/bin/bash".into());
        let lang = std::env::var("LANG")
            .ok()
            .filter(|l| l.to_lowercase().contains("utf"))
            .unwrap_or_else(|| "C.UTF-8".into());
        let env = [
            ("TERM", "xterm-256color".to_string()),
            ("COLORTERM", "truecolor".to_string()),
            ("LANG", lang),
            ("WT_MONITOR_TERMINAL", "1".to_string()),
        ];
        let mut p = pty::spawn(&shell, &["-l"], wt, &env, cols, rows)
            .map_err(|e| format!("não foi possível iniciar o terminal: {e}"))?;
        on_spawn(p.child.id());
        if kind == "claude" {
            let _ = p.master.write_all(b"claude\n"); // fica no shell quando o claude termina
        }
        let reader = p.master.try_clone().map_err(|e| e.to_string())?;
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let term = Arc::new(Term {
            id,
            path: wt.to_string_lossy().into_owned(),
            name: name.to_string(),
            kind,
            created,
            pid: p.child.id(),
            master: Mutex::new(p.master),
            alive: AtomicBool::new(true),
            exit: Mutex::new(None),
            died: Mutex::new(None),
            ring: Mutex::new(Vec::new()),
            subs: Mutex::new(Vec::new()),
        });
        let t = term.clone();
        thread::spawn(move || {
            let (mut rd, mut buf) = (reader, [0u8; 8192]);
            loop {
                match rd.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => t.push(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break, // EIO: todos os processos do terminal saíram
                }
            }
        });
        let t = term.clone();
        thread::spawn(move || {
            let code = match p.child.wait() {
                Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
                Err(_) => -1,
            };
            thread::sleep(Duration::from_millis(200)); // deixa a última saída chegar
            t.finish(code);
        });
        items.insert(id, term.clone());
        Ok(term)
    }

    fn list_json(&self) -> String {
        let mut items = self.items.lock().unwrap();
        self.sweep(&mut items);
        let mut v: Vec<&Arc<Term>> = items.values().collect();
        v.sort_by_key(|t| t.id);
        let list: Vec<String> = v
            .iter()
            .map(|t| {
                let exit = t.exit.lock().unwrap().map_or("null".to_string(), |c| c.to_string());
                format!(
                    "{{\"id\":{},\"path\":\"{}\",\"name\":\"{}\",\"kind\":\"{}\",\"alive\":{},\"exit\":{exit},\"t\":{:.0}}}",
                    t.id,
                    esc(&t.path),
                    esc(&t.name),
                    t.kind,
                    t.alive.load(Ordering::SeqCst),
                    t.created
                )
            })
            .collect();
        format!("[{}]", list.join(","))
    }
}

/// Rotas `/api/terminals*` e `/ws/term/<id>`. Todas respondem 404 quando os terminais estão desligados.
pub fn route(
    mut s: TcpStream,
    ctx: &Ctx,
    method: &str,
    route: &str,
    hdr: &dyn Fn(&str) -> Option<String>,
    body: &str,
) {
    let terms = &ctx.terms;
    if !terms.enabled {
        return respond(
            &mut s,
            "404 Not Found",
            "text/plain",
            "terminals are disabled (start with --terminals)",
        );
    }
    let authed = authed(&terms.token, hdr);
    if method == "GET" && route == "/api/terminals" {
        // Sem token, só se revela que os terminais existem: o token nunca é devolvido por esta API.
        let json = if authed {
            format!(
                "{{\"enabled\":true,\"auth\":true,\"terms\":{}}}",
                terms.list_json()
            )
        } else {
            "{\"enabled\":true,\"auth\":false}".to_string()
        };
        return respond(&mut s, "200 OK", "application/json", &json);
    }
    if let Some(id) = route.strip_prefix("/ws/term/") {
        return serve_ws(s, ctx, id.parse().unwrap_or(0), hdr);
    }
    if !authed {
        return respond(&mut s, "403 Forbidden", "text/plain", "forbidden");
    }
    match (method, route) {
        ("POST", "/api/terminals") => {
            let raw = qparam(body, "path").map(pct_decode).unwrap_or_default();
            let kind = match qparam(body, "kind") {
                None | Some("shell") => "shell",
                Some("claude") => "claude",
                Some(_) => {
                    return respond(
                        &mut s,
                        "400 Bad Request",
                        "text/plain",
                        "kind must be shell or claude",
                    )
                }
            };
            let Some(wt) = ctx.worktree(&raw) else {
                return respond(
                    &mut s,
                    "400 Bad Request",
                    "text/plain",
                    "path is not a worktree inside the watched root",
                );
            };
            let num = |k: &str, d: u16, hi: u16| {
                qparam(body, k)
                    .and_then(|v| v.parse::<u16>().ok())
                    .unwrap_or(d)
                    .clamp(1, hi)
            };
            let name = wt
                .strip_prefix(&ctx.root)
                .unwrap_or(&wt)
                .to_string_lossy()
                .into_owned();
            let size = (num("cols", 80, 500), num("rows", 24, 300));
            match terms.create(&wt, &name, kind, size, &|pid| {
                ctx.control.on_spawn(&wt, pid)
            }) {
                Ok(t) => respond(
                    &mut s,
                    "201 Created",
                    "application/json",
                    &format!("{{\"id\":{}}}", t.id),
                ),
                Err(e) => respond(
                    &mut s,
                    "429 Too Many Requests",
                    "application/json",
                    &format!("{{\"error\":\"{}\"}}", esc(&e)),
                ),
            }
        }
        ("DELETE", r) if r.starts_with("/api/terminals/") => {
            let id = r["/api/terminals/".len()..].parse().unwrap_or(0);
            if terms.remove(id) {
                respond(&mut s, "204 No Content", "text/plain", "")
            } else {
                respond(&mut s, "404 Not Found", "text/plain", "no such terminal")
            }
        }
        _ => respond(&mut s, "404 Not Found", "text/plain", "not found"),
    }
}

fn serve_ws(mut s: TcpStream, ctx: &Ctx, id: u32, hdr: &dyn Fn(&str) -> Option<String>) {
    // Navegadores sempre enviam Origin no handshake; um site de terceiros nunca coincide com o do painel.
    let origin_ok = hdr("origin").is_none_or(|o| {
        o == format!("http://127.0.0.1:{}", ctx.port)
            || o == format!("http://localhost:{}", ctx.port)
    });
    if !origin_ok {
        return respond(&mut s, "403 Forbidden", "text/plain", "forbidden origin");
    }
    let upgrade = hdr("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    let Some(key) = hdr("sec-websocket-key").filter(|_| upgrade) else {
        return respond(
            &mut s,
            "400 Bad Request",
            "text/plain",
            "websocket upgrade required",
        );
    };
    let Some(term) = ctx.terms.get(id) else {
        return respond(&mut s, "404 Not Found", "text/plain", "no such terminal");
    };
    if s.write_all(ws::handshake_response(&key).as_bytes())
        .is_err()
    {
        return;
    }
    session(s, &term, &ctx.terms.token);
}

/// `"chave":"valor"` simples (os valores que usamos nunca têm escapes).
fn json_str<'a>(p: &'a str, key: &str) -> Option<&'a str> {
    let rest = &p[p.find(&format!("\"{key}\":\""))? + key.len() + 4..];
    Some(&rest[..rest.find('"')?])
}

/// `"resize":[colunas,linhas]`
fn json_resize(p: &str) -> Option<(u16, u16)> {
    let rest = &p[p.find("\"resize\":[")? + 10..];
    let (a, b) = rest[..rest.find(']')?].split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// Protocolo: o cliente abre com `{"token":"…"}`; depois, binário = teclado, texto = `{"resize":[c,l]}`.
/// O servidor envia a saída acumulada, depois a saída ao vivo (binário) e `{"exit":N}` (texto) ao terminar.
fn session(mut rd: TcpStream, term: &Arc<Term>, token: &str) {
    let _ = rd.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(wr) = rd.try_clone() else { return };
    let wr = Arc::new(Mutex::new(wr));
    let first = ws::read_message(&mut rd);
    let authed = matches!(&first, Ok((ws::TEXT, p)) if json_str(&String::from_utf8_lossy(p), "token").is_some_and(|t| ct_eq(t, token)));
    if !authed {
        let _ = ws::write_frame(&mut *wr.lock().unwrap(), ws::CLOSE, &[0x03, 0xF0]); // 1008: violação de política
        return;
    }
    let _ = rd.set_read_timeout(None);

    let (snapshot, rx) = term.attach();
    if !snapshot.is_empty()
        && ws::write_frame(&mut *wr.lock().unwrap(), ws::BINARY, &snapshot).is_err()
    {
        return;
    }
    let pump = wr.clone();
    thread::spawn(move || {
        for m in rx {
            let mut w = pump.lock().unwrap();
            let sent = match m {
                Msg::Data(d) => ws::write_frame(&mut *w, ws::BINARY, &d),
                Msg::Exit(c) => {
                    ws::write_frame(&mut *w, ws::TEXT, format!("{{\"exit\":{c}}}").as_bytes())
                }
            };
            if sent.is_err() {
                break;
            }
        }
    });
    while let Ok((op, p)) = ws::read_message(&mut rd) {
        match op {
            ws::BINARY => {
                if term.write_input(&p).is_err() {
                    break;
                }
            }
            ws::TEXT => {
                if let Some((c, l)) = json_resize(&String::from_utf8_lossy(&p)) {
                    term.resize(c, l);
                }
            }
            ws::PING => {
                let _ = ws::write_frame(&mut *wr.lock().unwrap(), ws::PONG, &p);
            }
            ws::CLOSE => break,
            _ => {}
        }
    }
    let _ = wr.lock().unwrap().shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_json_helpers() {
        assert_eq!(json_str("{\"token\":\"abc123\"}", "token"), Some("abc123"));
        assert_eq!(json_str("{\"x\":1}", "token"), None);
        assert_eq!(json_resize("{\"resize\":[120,40]}"), Some((120, 40)));
        assert_eq!(json_resize("{\"resize\":[a,40]}"), None);
    }

    #[test]
    fn constant_time_compare() {
        assert!(ct_eq("abc", "abc"));
        assert!(!ct_eq("abc", "abd"));
        assert!(!ct_eq("abc", "abcd"));
    }

    #[test]
    fn token_file_is_private_and_stable() {
        let dir = std::env::temp_dir().join(format!("wt-monitor-token-{}", std::process::id()));
        let file = dir.join("sub/token");
        std::env::set_var("WT_TOKEN_FILE", &file);
        let a = load_or_create_token().unwrap();
        let b = load_or_create_token().unwrap();
        assert_eq!(a, b, "o token persiste entre execuções");
        assert_eq!(a.len(), 64);
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600,
            "arquivo só do dono"
        );
        assert_eq!(
            fs::metadata(file.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700,
            "pasta só do dono"
        );
        std::env::remove_var("WT_TOKEN_FILE");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (random_token().unwrap(), random_token().unwrap());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
