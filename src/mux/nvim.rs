use nix::fcntl::{FcntlArg, OFlag, fcntl};
use std::env;
use std::io::{self, Error, Read};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::daemon::{DaemonContext, LoadOutcome};

const RPC_TIMEOUT: Duration = Duration::from_secs(5);
const RPC_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) fn socket() -> Option<String> {
    if env::var("DIRENV_INSTANT_NVIM").as_deref() == Ok("1") {
        env::var("NVIM").ok()
    } else {
        None
    }
}

enum Event<'a> {
    Watch(&'a str),
    Finished(LoadOutcome),
}

pub(super) fn spawn(bin: &str, ctx: &DaemonContext) -> io::Result<()> {
    let mut child = spawn_request(ctx, Event::Watch(bin))?;
    thread::spawn(move || {
        if let Err(err) = wait_for_reply(&mut child, RPC_TIMEOUT) {
            eprintln!("direnv-instant: Failed to report load progress: {err}");
        }
    });
    Ok(())
}

pub(super) fn finish(ctx: &DaemonContext, outcome: LoadOutcome) -> io::Result<()> {
    wait_for_reply(
        &mut spawn_request(ctx, Event::Finished(outcome))?,
        RPC_TIMEOUT,
    )
}

fn expression(ctx: &DaemonContext, event: Event<'_>) -> String {
    let (method, extra) = match event {
        Event::Watch(bin) => ("watch", format!(r#", bin="{}""#, escape_lua_string(bin))),
        Event::Finished(outcome) => {
            let (status, code) = match outcome {
                LoadOutcome::Exited(0) => ("success", 0),
                LoadOutcome::Exited(130) => ("cancel", 130),
                LoadOutcome::Exited(code) => ("failed", code),
                LoadOutcome::Cancelled => ("cancel", 143),
            };
            (
                "finish_watch",
                format!(r#", status="{status}", code={code}"#),
            )
        }
    };
    let lua = format!(
        r#"require("mux.direnv").{method}({{log="{}", socket="{}", shell_pid={}, target="{}"{extra}}})"#,
        escape_lua_string(&ctx.temp_stderr.to_string_lossy()),
        escape_lua_string(&ctx.socket_path.to_string_lossy()),
        ctx.parent_pid,
        escape_lua_string(&ctx.envrc_dir.to_string_lossy()),
    );
    format!("luaeval('{}') ? 1 : 0", lua.replace('\'', "''"))
}

fn spawn_request(ctx: &DaemonContext, event: Event<'_>) -> io::Result<Child> {
    let socket = socket().ok_or_else(|| Error::other("Neovim mux socket is unavailable"))?;
    Command::new("nvim")
        .args([
            "--server",
            &socket,
            "--remote-expr",
            &expression(ctx, event),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

fn wait_for_reply(child: &mut Child, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    let result = (|| {
        loop {
            if let Some(status) = child.try_wait()? {
                if !status.success() {
                    return Err(Error::other("Neovim notification failed"));
                }
                let stdout = child
                    .stdout
                    .as_mut()
                    .ok_or_else(|| Error::other("Neovim reply is unavailable"))?;
                let flags = OFlag::from_bits_truncate(fcntl(&*stdout, FcntlArg::F_GETFL)?);
                fcntl(&*stdout, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
                let mut reply = String::new();
                stdout.take(64).read_to_string(&mut reply)?;
                return if reply.trim() == "1" {
                    Ok(())
                } else {
                    Err(Error::other("Neovim rejected notification"))
                };
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    io::ErrorKind::TimedOut,
                    "Neovim notification timed out",
                ));
            }
            thread::sleep(RPC_POLL_INTERVAL);
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

fn escape_lua_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_ascii_control() => escaped.push_str(&format!("\\{:03}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::Shell;

    fn context() -> DaemonContext {
        DaemonContext {
            parent_pid: 42,
            envrc_dir: "/project's root".into(),
            runtime_dir: "/runtime".into(),
            socket_path: "/runtime/daemon.sock".into(),
            env_file: "/runtime/env".into(),
            stderr_file: "/runtime/env.stderr".into(),
            temp_file: "/runtime/env.123".into(),
            temp_stderr: "/runtime/log.123".into(),
            multiplexer: Some(super::super::Multiplexer::Nvim),
            shell: Shell::Zsh,
        }
    }

    fn reply(script: &str) -> Child {
        Command::new("bash")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn quotes_lua_strings_without_ambiguous_decimal_escapes() {
        assert_eq!(
            escape_lua_string("\"\\\n\r\t\x011é"),
            "\\\"\\\\\\n\\r\\t\\0011é"
        );
    }

    #[test]
    fn watch_and_finish_share_the_operation_identity() {
        let ctx = context();
        for event in [
            Event::Watch("/bin/direnv-instant"),
            Event::Finished(LoadOutcome::Exited(0)),
        ] {
            let expr = expression(&ctx, event);
            assert!(expr.contains(r#"log="/runtime/log.123", socket="/runtime/daemon.sock", shell_pid=42, target="/project''s root""#));
            assert!(expr.ends_with("') ? 1 : 0"));
        }
    }

    #[test]
    fn completion_status_is_derived_from_the_load_outcome() {
        for (outcome, status, code) in [
            (LoadOutcome::Exited(0), "success", 0),
            (LoadOutcome::Exited(7), "failed", 7),
            (LoadOutcome::Exited(130), "cancel", 130),
            (LoadOutcome::Exited(143), "failed", 143),
            (LoadOutcome::Cancelled, "cancel", 143),
        ] {
            let expr = expression(&context(), Event::Finished(outcome));
            assert!(expr.contains(&format!(r#"status="{status}", code={code}"#)));
        }
    }

    #[test]
    fn accepts_an_acknowledged_notification() {
        assert!(wait_for_reply(&mut reply("printf '1\\n'"), Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn rejects_an_unacknowledged_notification() {
        for script in ["printf '0\\n'", "exit 0", "printf 'unexpected\\n'"] {
            assert!(wait_for_reply(&mut reply(script), Duration::from_secs(1)).is_err());
        }
    }

    #[test]
    fn rejects_a_failed_client_even_with_an_acknowledgement() {
        assert!(
            wait_for_reply(&mut reply("printf '1\\n'; exit 3"), Duration::from_secs(1)).is_err()
        );
    }

    #[test]
    fn reaps_a_timed_out_client() {
        let mut child = reply("exec sleep 60");
        let result = wait_for_reply(&mut child, Duration::from_millis(20));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(child.try_wait().unwrap().is_some());
    }
}
