mod common;
use common::*;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn watcher_and_completion_identify_the_same_load() {
    let sb = Sandbox::new(
        "echo loading >&2\nfor i in {1..100}; do [ -f \"$HOME/release\" ] && break; sleep 0.05; done\n[ -f \"$HOME/release\" ] || exit 1\nexport NVIM_TEST_VALUE=loaded\n",
    )
    .unwrap();
    sb.write_stub(
        "nvim",
        "printf '%s\\n' \"$@\" >> \"$HOME/events\"\ncase \"$4\" in *'.watch('*) touch \"$HOME/release\" ;; esac\nprintf '1\\n'",
    )
    .unwrap();
    let sink = SignalSink::new().unwrap();
    let mut env = sb.async_env(sink.pid(), 0);
    env.insert("DIRENV_INSTANT_NVIM".into(), "1".into());
    env.insert("NVIM".into(), "editor".into());
    let output = sb.run(&["start"], &env).unwrap();
    assert!(output.status.success());
    let exports = parse_exports(&String::from_utf8_lossy(&output.stdout));
    let env_file = PathBuf::from(&exports["__DIRENV_INSTANT_ENV_FILE"]);
    assert!(wait_for_file(&env_file, Duration::from_secs(10)));
    assert!(wait_for_daemon_exit(
        &env_file.parent().unwrap().join("daemon.sock"),
        Duration::from_secs(10),
    ));
    let events = fs::read_to_string(sb.home.join("events")).unwrap();
    let watch = events
        .lines()
        .find(|line| line.contains(".watch("))
        .unwrap();
    let finish = events
        .lines()
        .find(|line| line.contains(".finish_watch("))
        .unwrap();
    for key in ["log", "socket", "target"] {
        let prefix = format!("{key}=\"");
        let field = |event: &str| {
            event
                .split_once(&prefix)
                .unwrap()
                .1
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        assert_eq!(field(watch), field(finish));
    }
    assert!(watch.contains("bin=\""));
    assert!(finish.contains("status=\"success\""));
}

#[test]
fn rejected_notification_does_not_discard_the_export() {
    let sb = Sandbox::new("export NVIM_TEST_VALUE=loaded\n").unwrap();
    sb.write_stub("nvim", "printf '0\\n'").unwrap();
    let sink = SignalSink::new().unwrap();
    let mut env = sb.async_env(sink.pid(), 60);
    env.insert("DIRENV_INSTANT_NVIM".into(), "1".into());
    env.insert("NVIM".into(), "editor".into());
    let log = sb.home.join("daemon.log");
    env.insert(
        "DIRENV_INSTANT_DEBUG_LOG".into(),
        log.clone().into_os_string(),
    );
    let output = sb.run(&["start"], &env).unwrap();
    assert!(output.status.success());
    let exports = parse_exports(&String::from_utf8_lossy(&output.stdout));
    let env_file = PathBuf::from(&exports["__DIRENV_INSTANT_ENV_FILE"]);
    assert!(wait_for_file(&env_file, Duration::from_secs(10)));
    assert!(wait_for_daemon_exit(
        &env_file.parent().unwrap().join("daemon.sock"),
        Duration::from_secs(10),
    ));
    assert!(
        fs::read_to_string(&env_file)
            .unwrap()
            .contains("NVIM_TEST_VALUE")
    );
    assert!(
        fs::read_to_string(log)
            .unwrap()
            .contains("Neovim rejected notification")
    );
}

#[test]
fn nvim_sessions_have_independent_daemons_and_completion_events() {
    let sb = Sandbox::new("export NVIM_TEST_VALUE=loaded\n").unwrap();
    sb.write_stub_tmux("touch \"$HOME/unexpected-tmux\"")
        .unwrap();
    sb.write_stub(
        "nvim",
        "printf '%s\\n' \"$@\" >> \"$HOME/$NVIM\"\nprintf '1\\n'",
    )
    .unwrap();
    let sink = SignalSink::new().unwrap();
    let mut env = sb.async_env(sink.pid(), 60);
    env.insert("DIRENV_INSTANT_NVIM".into(), "1".into());
    let mut environments = Vec::new();

    for session in ["first", "second"] {
        env.insert("NVIM".into(), session.into());
        let output = sb.run(&["start"], &env).unwrap();
        assert!(output.status.success());
        let exports = parse_exports(&String::from_utf8_lossy(&output.stdout));
        let env_file = PathBuf::from(&exports["__DIRENV_INSTANT_ENV_FILE"]);
        assert!(wait_for_file(&env_file, Duration::from_secs(10)));
        assert!(wait_for_daemon_exit(
            &env_file.parent().unwrap().join("daemon.sock"),
            Duration::from_secs(10),
        ));
        assert!(
            fs::read_to_string(&env_file)
                .unwrap()
                .contains("NVIM_TEST_VALUE")
        );
        let event = fs::read_to_string(sb.home.join(session)).unwrap();
        assert!(event.contains("require(\"mux.direnv\").finish_watch("));
        assert!(event.contains("status=\"success\""));
        assert!(event.contains("code=0"));
        assert!(event.contains(&format!("shell_pid={}", sink.pid())));
        environments.push(env_file);
    }

    assert_ne!(environments[0], environments[1]);
    assert!(!sb.home.join("unexpected-tmux").exists());
}

#[test]
fn nvim_reports_failure_without_opening_a_watcher() {
    let sb = Sandbox::new("echo failed-load >&2\nexit 7\n").unwrap();
    sb.write_stub(
        "nvim",
        "printf '%s\\n' \"$@\" >> \"$HOME/events\"\nprintf '1\\n'",
    )
    .unwrap();
    let sink = SignalSink::new().unwrap();
    let mut env = sb.async_env(sink.pid(), 60);
    env.remove(std::ffi::OsStr::new("TMUX"));
    env.insert("DIRENV_INSTANT_NVIM".into(), "1".into());
    env.insert("NVIM".into(), "editor".into());
    let output = sb.run(&["start"], &env).unwrap();
    assert!(output.status.success());
    let exports = parse_exports(&String::from_utf8_lossy(&output.stdout));
    let stderr_file = PathBuf::from(&exports["__DIRENV_INSTANT_STDERR_FILE"]);
    assert!(wait_for_file(&stderr_file, Duration::from_secs(10)));
    assert!(wait_for_daemon_exit(
        &stderr_file.parent().unwrap().join("daemon.sock"),
        Duration::from_secs(10),
    ));
    let event = fs::read_to_string(sb.home.join("events")).unwrap();
    assert!(event.contains(".finish_watch("));
    assert!(!event.contains(".watch("));
    assert!(event.contains("status=\"failed\""));
    assert!(!event.contains("code=0"));
    assert!(
        fs::read_to_string(stderr_file)
            .unwrap()
            .contains("failed-load")
    );
}
