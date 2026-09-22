//! Game process management (§25, §26): spawn the JVM, stream its output as
//! bounded events, and classify how it exited.

use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use crate::launch::LaunchSpec;

/// One line of game output.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub stderr: bool,
    pub text: String,
}

/// Why the game process ended — the distinction §25 asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum ExitClass {
    /// Clean shutdown (code 0).
    Normal,
    /// The JVM itself refused to start or died at VM level (bad arguments,
    /// unsupported class version, OOM at VM level).
    JavaError { detail: String },
    /// The game started and then crashed.
    MinecraftCrash { detail: String },
    /// A mod was named in the failure output.
    ModError { detail: String },
    /// We could not even spawn the process.
    LauncherError { detail: String },
    /// Terminated by the user through the launcher.
    Killed,
}

#[derive(Debug, Clone)]
pub struct ExitReport {
    pub code: Option<i32>,
    pub class: ExitClass,
}

/// A running game process.
pub struct GameProcess {
    child: tokio::process::Child,
    killed: Arc<AtomicBool>,
    /// Recent output lines, used to classify a failure exit.
    recent: Arc<tokio::sync::Mutex<Vec<String>>>,
}

/// How many recent lines to retain for crash classification. Bounded so a
/// spammy log can never grow memory without limit (§26).
const RECENT_LINES: usize = 200;

impl GameProcess {
    /// Spawn the game. Output lines are sent to `log_tx`; the channel being
    /// full or closed never blocks the game (lines are dropped instead).
    pub fn spawn(spec: &LaunchSpec, log_tx: mpsc::Sender<LogLine>) -> std::io::Result<Self> {
        let mut command = tokio::process::Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        #[cfg(windows)]
        {
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let mut child = command.spawn()?;
        let recent = Arc::new(tokio::sync::Mutex::new(Vec::new()));

        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, false, log_tx.clone(), Arc::clone(&recent));
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, true, log_tx, Arc::clone(&recent));
        }

        Ok(Self {
            child,
            killed: Arc::new(AtomicBool::new(false)),
            recent,
        })
    }

    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Ask the OS to terminate the game.
    pub async fn kill(&mut self) -> std::io::Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        self.child.kill().await
    }

    /// Wait for exit and classify the result.
    pub async fn wait(mut self) -> ExitReport {
        let status = match self.child.wait().await {
            Ok(status) => status,
            Err(e) => {
                return ExitReport {
                    code: None,
                    class: ExitClass::LauncherError {
                        detail: e.to_string(),
                    },
                }
            }
        };
        let code = status.code();
        if self.killed.load(Ordering::SeqCst) {
            return ExitReport {
                code,
                class: ExitClass::Killed,
            };
        }
        if status.success() {
            return ExitReport {
                code,
                class: ExitClass::Normal,
            };
        }
        let recent = self.recent.lock().await.clone();
        ExitReport {
            code,
            class: classify(&recent),
        }
    }
}

fn spawn_reader<R>(
    reader: R,
    stderr: bool,
    tx: mpsc::Sender<LogLine>,
    recent: Arc<tokio::sync::Mutex<Vec<String>>>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(text)) = lines.next_line().await {
            {
                let mut buf = recent.lock().await;
                if buf.len() == RECENT_LINES {
                    buf.remove(0);
                }
                buf.push(text.clone());
            }
            // try_send: never let a slow console consumer stall the game.
            let _ = tx.try_send(LogLine { stderr, text });
        }
    });
}

/// Classify a failure from the tail of the process output.
pub fn classify(lines: &[String]) -> ExitClass {
    let haystack = lines.join("\n");
    let lower = haystack.to_lowercase();

    // A named mod in the failure is the most specific signal.
    for line in lines.iter().rev() {
        let l = line.to_lowercase();
        if l.contains("mod ")
            && (l.contains("requires") || l.contains("incompatible") || l.contains("missing"))
            || l.contains("mixin apply failed")
            || l.contains("fabric loader") && l.contains("error")
            || l.contains("incompatible mod set")
        {
            return ExitClass::ModError {
                detail: line.trim().to_string(),
            };
        }
    }

    // JVM-level failures: these appear before the game ever starts.
    const JAVA_MARKERS: [&str; 6] = [
        "unrecognized option",
        "unsupportedclassversionerror",
        "could not create the java virtual machine",
        "error: could not find or load main class",
        "unable to initialize main class",
        "invalid maximum heap size",
    ];
    if let Some(marker) = JAVA_MARKERS.iter().find(|m| lower.contains(**m)) {
        let detail = lines
            .iter()
            .rev()
            .find(|l| l.to_lowercase().contains(*marker))
            .cloned()
            .unwrap_or_else(|| (*marker).to_string());
        return ExitClass::JavaError { detail };
    }

    // Otherwise: the game itself failed.
    let detail = lines
        .iter()
        .rev()
        .find(|l| {
            let l = l.to_lowercase();
            l.contains("exception") || l.contains("error") || l.contains("crash")
        })
        .cloned()
        .unwrap_or_else(|| "the game exited with a non-zero status".to_string());
    ExitClass::MinecraftCrash {
        detail: detail.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_jvm_argument_failure_as_java_error() {
        let lines = vec![
            "Unrecognized option: -XX:+TotallyFakeFlag".to_string(),
            "Error: Could not create the Java Virtual Machine.".to_string(),
        ];
        match classify(&lines) {
            ExitClass::JavaError { detail } => assert!(detail.contains("Unrecognized option")),
            other => panic!("expected JavaError, got {other:?}"),
        }
    }

    #[test]
    fn classifies_unsupported_class_version_as_java_error() {
        let lines = vec![
            "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version".to_string(),
        ];
        assert!(matches!(classify(&lines), ExitClass::JavaError { .. }));
    }

    #[test]
    fn classifies_mod_failure_as_mod_error() {
        let lines = vec![
            "[main/INFO]: Loading 42 mods".to_string(),
            "Mod sodium requires version 0.5.0 of fabric-api, which is missing!".to_string(),
        ];
        match classify(&lines) {
            ExitClass::ModError { detail } => assert!(detail.contains("sodium")),
            other => panic!("expected ModError, got {other:?}"),
        }
    }

    #[test]
    fn classifies_game_crash_as_minecraft_crash() {
        let lines = vec![
            "[Render thread/INFO]: Setting user: Faerie".to_string(),
            "java.lang.NullPointerException: Cannot invoke render()".to_string(),
        ];
        match classify(&lines) {
            ExitClass::MinecraftCrash { detail } => {
                assert!(detail.contains("NullPointerException"))
            }
            other => panic!("expected MinecraftCrash, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn spawning_a_missing_program_is_an_io_error() {
        let spec = LaunchSpec {
            program: "definitely-not-a-real-program-xyz".into(),
            args: vec![],
            cwd: std::env::temp_dir(),
            env: vec![],
        };
        let (tx, _rx) = mpsc::channel(4);
        assert!(GameProcess::spawn(&spec, tx).is_err());
    }
}
