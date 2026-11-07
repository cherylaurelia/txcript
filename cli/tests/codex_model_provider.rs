#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Codex 0.166 and later refuse to resume a rollout whose
//! `session_meta.model_provider` is not the provider Codex runs with:
//! "This chat uses openai, but your current provider is ollama-local". Every
//! copy txcript wrote said `openai`, so `continue --with codex`, and even a
//! crop of a native Codex session, could not be resumed by anyone whose
//! `config.toml` selects another provider. Runs the real `txcript` binary
//! against temp stores; every other store is pointed at an empty temp dir.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CLAUDE_ID: &str = "22222222-aaaa-4aaa-8aaa-000000000001";
const CODEX_ID: &str = "22222222-bbbb-4bbb-8bbb-000000000002";

struct Env {
    home: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        let env = Self {
            home: tempfile::tempdir().unwrap(),
        };
        let project = env.home.path().join(".claude/projects/-work-proj");
        fs::create_dir_all(&project).unwrap();
        let lines = [
            format!(
                r#"{{"type":"user","uuid":"u1","parentUuid":null,"sessionId":"{CLAUDE_ID}","cwd":"/work/proj","timestamp":"2026-11-01T00:00:01.000Z","message":{{"role":"user","content":"name the button"}}}}"#
            ),
            format!(
                r#"{{"type":"assistant","uuid":"a1","parentUuid":"u1","sessionId":"{CLAUDE_ID}","cwd":"/work/proj","timestamp":"2026-11-01T00:00:02.000Z","message":{{"role":"assistant","model":"claude-sonnet-5-5","content":[{{"type":"text","text":"Cancel"}}]}}}}"#
            ),
        ];
        fs::write(
            project.join(format!("{CLAUDE_ID}.jsonl")),
            lines.join("\n") + "\n",
        )
        .unwrap();
        fs::create_dir_all(env.sessions()).unwrap();
        env
    }

    fn codex_home(&self) -> PathBuf {
        self.home.path().join("codex-home")
    }

    fn sessions(&self) -> PathBuf {
        self.codex_home().join("sessions")
    }

    fn config(&self, toml: &str) {
        fs::write(self.codex_home().join("config.toml"), toml).unwrap();
    }

    /// A native rollout, as Codex writes it under a custom provider.
    fn native_codex_session(&self, provider: &str) {
        let day = self.sessions().join("2026/11/02");
        fs::create_dir_all(&day).unwrap();
        let lines = [
            format!(
                r#"{{"timestamp":"2026-11-02T00:00:00.000Z","type":"session_meta","payload":{{"id":"{CODEX_ID}","timestamp":"2026-11-02T00:00:00.000Z","cwd":"/work/proj","originator":"codex_exec","cli_version":"0.167.0","source":"exec","model_provider":"{provider}"}}}}"#
            ),
            r#"{"timestamp":"2026-11-02T00:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}}"#.to_string(),
            r#"{"timestamp":"2026-11-02T00:00:02.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"}]}}"#.to_string(),
        ];
        fs::write(
            day.join(format!("rollout-2026-11-02T00-00-00-{CODEX_ID}.jsonl")),
            lines.join("\n") + "\n",
        )
        .unwrap();
    }

    fn run(&self, args: &[&str]) -> String {
        let empty = self.home.path().join("empty");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_txcript"));
        cmd.env_clear()
            .env("HOME", self.home.path())
            .env("USERPROFILE", self.home.path())
            .env("CLAUDE_CONFIG_DIR", self.home.path().join(".claude"))
            .env("CODEX_HOME", self.codex_home());
        for var in [
            "XDG_DATA_HOME",
            "XDG_CONFIG_HOME",
            "PI_CODING_AGENT_DIR",
            "OPENCODE_DB",
            "HERMES_HOME",
            "FX_HOME",
            "GROK_HOME",
            "COWORK_SESSIONS_DIR",
            "AMP_THREADS_DIR",
            "CURSOR_DESKTOP_USER_DIR",
            "TXCRIPT_GROK_BOT_ROOT",
        ] {
            cmd.env(var, &empty);
        }
        let out = cmd.args(args).output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "{text}");
        text
    }
}

/// `model_provider` of every rollout txcript wrote (any id but the native one).
fn written_providers(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "jsonl")
                && !path.to_string_lossy().contains(CODEX_ID)
            {
                let text = fs::read_to_string(&path).unwrap();
                let meta: serde_json::Value =
                    serde_json::from_str(text.lines().next().unwrap()).unwrap();
                assert_eq!(meta["type"], "session_meta");
                found.push(
                    meta["payload"]["model_provider"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                );
            }
        }
    }
    found
}

const OLLAMA_CONFIG: &str = r#"model = "qwen3-4b-local:latest"
model_provider = "ollama-local"

[model_providers.ollama-local]
name = "Ollama (local)"
base_url = "http://localhost:11434/v1"
"#;

#[test]
fn continue_into_codex_names_the_configured_provider() {
    let env = Env::new();
    env.config(OLLAMA_CONFIG);
    env.run(&["continue", CLAUDE_ID, "--with", "codex", "--no-resume"]);
    assert_eq!(written_providers(&env.sessions()), ["ollama-local"]);
}

#[test]
fn a_crop_of_a_native_codex_session_keeps_it_resumable() {
    let env = Env::new();
    env.config(OLLAMA_CONFIG);
    env.native_codex_session("ollama-local");
    env.run(&["continue", &format!("{CODEX_ID}#1-2"), "--no-resume"]);
    assert_eq!(written_providers(&env.sessions()), ["ollama-local"]);
}

#[test]
fn an_out_export_keeps_the_default() {
    let env = Env::new();
    env.config(OLLAMA_CONFIG);
    let out = env.home.path().join("export");
    env.run(&[
        "continue",
        CLAUDE_ID,
        "--with",
        "codex",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(written_providers(&out), ["openai"]);
}
