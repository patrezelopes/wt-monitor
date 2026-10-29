//! Pseudo-terminais por FFI direta com a libc (que a `std` já liga): sem crates externas. Só Linux.

use std::ffi::CStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::raw::{c_char, c_int, c_ulong};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

extern "C" {
    fn posix_openpt(flags: c_int) -> c_int;
    fn grantpt(fd: c_int) -> c_int;
    fn unlockpt(fd: c_int) -> c_int;
    fn ptsname_r(fd: c_int, buf: *mut c_char, len: usize) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn setsid() -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
    fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
}

// Constantes do Linux (iguais em x86_64 e aarch64).
const O_RDWR: c_int = 2;
const O_NOCTTY: c_int = 0o400;
const TIOCSWINSZ: c_ulong = 0x5414;
const TIOCSCTTY: c_ulong = 0x540E;
const F_SETFD: c_int = 2;
const FD_CLOEXEC: c_int = 1;
pub const SIGHUP: i32 = 1;
pub const SIGKILL: i32 = 9;

#[repr(C)]
struct Winsize {
    rows: u16,
    cols: u16,
    xpixel: u16,
    ypixel: u16,
}

fn check(r: c_int) -> io::Result<c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

pub struct Pty {
    /// Lado mestre: o que o processo escreve sai daqui; o que escrevemos vira teclado dele.
    pub master: File,
    pub child: Child,
}

pub fn resize(master: &File, cols: u16, rows: u16) -> io::Result<()> {
    let ws = Winsize {
        rows,
        cols,
        xpixel: 0,
        ypixel: 0,
    };
    // SAFETY: `ws` vive durante a chamada e TIOCSWINSZ só lê a estrutura.
    check(unsafe { ioctl(master.as_raw_fd(), TIOCSWINSZ, &ws as *const Winsize) }).map(|_| ())
}

/// Inicia `program` num pseudo-terminal próprio, como líder de sessão, com `cwd` e `env` extras.
pub fn spawn(
    program: &str,
    args: &[&str],
    cwd: &Path,
    env: &[(&str, String)],
    cols: u16,
    rows: u16,
) -> io::Result<Pty> {
    // SAFETY: chamadas diretas da libc; cada retorno é verificado e o fd mestre vira um `File` (fecha ao sair de escopo).
    let fd = check(unsafe { posix_openpt(O_RDWR | O_NOCTTY) })?;
    let master = unsafe { File::from_raw_fd(fd) };
    check(unsafe { fcntl(fd, F_SETFD, FD_CLOEXEC) })?; // o mestre não vaza para os processos filhos
    check(unsafe { grantpt(fd) })?;
    check(unsafe { unlockpt(fd) })?;
    let mut name = [0 as c_char; 128];
    let rc = unsafe { ptsname_r(fd, name.as_mut_ptr(), name.len()) };
    if rc != 0 {
        return Err(io::Error::from_raw_os_error(rc));
    }
    let path = unsafe { CStr::from_ptr(name.as_ptr()) }
        .to_str()
        .map_err(|_| io::Error::other("nome de pty inválido"))?
        .to_owned();
    resize(&master, cols, rows)?;

    // O_NOCTTY: o servidor não pode adotar este terminal como o seu terminal de controle.
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(O_NOCTTY)
        .open(&path)?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    for (k, v) in env {
        cmd.env(k, v);
    }
    // SAFETY: só chama funções async-signal-safe (setsid, ioctl) entre o fork e o exec.
    unsafe {
        cmd.pre_exec(|| {
            check(setsid())?;
            check(ioctl(0, TIOCSCTTY, 0 as c_int))?;
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    Ok(Pty { master, child })
}

/// Envia um sinal a todo o grupo de processos da sessão (o líder tem pgid == pid).
pub fn signal_group(pid: u32, sig: i32) {
    // SAFETY: `kill` com pid negativo sinaliza um grupo; o resultado é irrelevante se ele já terminou.
    unsafe {
        kill(-(pid as c_int), sig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn runs_a_command_on_a_pty_and_reads_its_output() {
        let mut p = spawn(
            "/bin/sh",
            &["-c", "printf 'oi %s' \"$TERM\"; stty size"],
            Path::new("/"),
            &[("TERM", "xterm-256color".into())],
            100,
            30,
        )
        .unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        // ao terminar, ler o mestre devolve EIO: é o sinal de fim
        while let Ok(n) = p.master.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        assert!(p.child.wait().unwrap().success());
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("oi xterm-256color"), "{text:?}");
        assert!(text.contains("30 100"), "{text:?}"); // linhas colunas
    }
}
