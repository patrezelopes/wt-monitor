//! Abre um worktree no editor instalado na máquina (VS Code, Cursor…). Veja SECURITY.md.
//! O comando vem só de `WT_EDITOR` ou da lista fixa abaixo, nunca da requisição.

use crate::{pct_decode, qparam, respond, terms, Ctx};
use std::net::TcpStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Procurados no PATH, na ordem, quando `WT_EDITOR` não está definido.
const CANDIDATES: [&str; 5] = ["code", "cursor", "codium", "windsurf", "zed"];

/// Diretórios extras: o PATH de um serviço de usuário costuma ser mais curto que o do terminal.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    dirs.push(PathBuf::from(format!("{home}/.local/bin")));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/snap/bin"));
    dirs
}

fn find_in_path(prog: &str) -> Option<PathBuf> {
    if prog.contains('/') {
        return PathBuf::from(prog).is_file().then(|| PathBuf::from(prog));
    }
    search_dirs()
        .into_iter()
        .map(|d| d.join(prog))
        .find(|p| p.is_file())
}

/// Comando do editor (programa + argumentos fixos) ou `None` se não houver nenhum instalado.
pub fn detect() -> Option<Vec<String>> {
    if let Ok(v) = std::env::var("WT_EDITOR") {
        let argv: Vec<String> = v.split_whitespace().map(String::from).collect();
        return argv.first().and_then(|p| find_in_path(p)).map(|full| {
            std::iter::once(full.to_string_lossy().into_owned())
                .chain(argv.into_iter().skip(1))
                .collect()
        });
    }
    CANDIDATES
        .iter()
        .find_map(|c| find_in_path(c))
        .map(|p| vec![p.to_string_lossy().into_owned()])
}

fn label(argv: &[String]) -> String {
    argv.first()
        .and_then(|p| p.rsplit('/').next())
        .unwrap_or("")
        .to_string()
}

/// Rotas `/api/open`. GET diz se há editor e se este navegador está autorizado; POST abre o worktree.
pub fn route(
    mut s: TcpStream,
    ctx: &Ctx,
    method: &str,
    hdr: &dyn Fn(&str) -> Option<String>,
    body: &str,
) {
    // Sem terminais nem controle não existe token; um token vazio nunca autoriza.
    let authed = !ctx.token.is_empty() && terms::authed(&ctx.token, hdr);
    let editor = detect();
    if method == "GET" {
        let name = editor.as_deref().map(label).unwrap_or_default();
        let json = format!(
            "{{\"enabled\":{},\"auth\":{authed},\"editor\":\"{}\"}}",
            !ctx.token.is_empty(),
            crate::esc(&name)
        );
        return respond(&mut s, "200 OK", "application/json", &json);
    }
    if method != "POST" {
        return respond(&mut s, "404 Not Found", "text/plain", "not found");
    }
    if !authed {
        return respond(&mut s, "403 Forbidden", "text/plain", "forbidden");
    }
    let raw = qparam(body, "path").map(pct_decode).unwrap_or_default();
    let Some(wt) = ctx.worktree(&raw) else {
        return respond(
            &mut s,
            "400 Bad Request",
            "text/plain",
            "path is not a worktree inside the watched root",
        );
    };
    let Some(argv) = editor else {
        return respond(
            &mut s,
            "503 Service Unavailable",
            "text/plain",
            "no editor found (install VS Code or set WT_EDITOR)",
        );
    };
    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .arg(&wt)
        .current_dir(&wt)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0) // não morre junto com o painel
        .spawn();
    match child {
        Ok(mut c) => {
            std::thread::spawn(move || {
                let _ = c.wait(); // evita zumbi
            });
            respond(&mut s, "204 No Content", "text/plain", "")
        }
        Err(e) => respond(
            &mut s,
            "500 Internal Server Error",
            "text/plain",
            &format!("could not start the editor: {e}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_is_the_program_name() {
        assert_eq!(label(&["/usr/bin/code".into(), "-n".into()]), "code");
        assert_eq!(label(&[]), "");
    }

    #[test]
    fn missing_program_is_not_found() {
        assert!(find_in_path("definitely-not-an-editor-xyz").is_none());
        assert!(find_in_path("/nonexistent/editor").is_none());
        assert!(find_in_path("sh").is_some());
    }
}
