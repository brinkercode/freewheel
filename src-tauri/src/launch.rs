//! The production [`Launcher`]: spawns the program directly with an argv
//! array (`Command::new`), never through a shell, so arguments are passed to
//! the OS unchanged and never interpreted (threat T2).

use std::process::Command;

use freewheel_core::launch::{LaunchError, Launcher};

pub struct ProcessLauncher;

impl Launcher for ProcessLauncher {
    fn launch(&mut self, path: &str, args: &[String]) -> Result<(), LaunchError> {
        self.launch_in(path, args, None)
    }

    /// `cwd`: the folder the program starts in (a mappings file's `cwd`).
    fn launch_in(
        &mut self,
        path: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<(), LaunchError> {
        let mut command = Command::new(path);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        command
            .spawn()
            .map(drop)
            .map_err(|e| LaunchError(format!("{path}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_program_is_an_error_not_a_panic() {
        let err = ProcessLauncher
            .launch("/nonexistent/freewheel-test-app", &["$(id)".into()])
            .unwrap_err();
        assert!(err.0.starts_with("/nonexistent/freewheel-test-app: "));
    }

    #[cfg(unix)]
    #[test]
    fn a_program_starts_in_its_cwd_with_its_args() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.txt");
        let script = format!(
            "pwd > '{}'; echo \"$1|$2\" >> '{}'",
            out.display(),
            out.display()
        );
        ProcessLauncher
            .launch_in(
                "/bin/sh",
                &["-c".into(), script, "sh".into(), "a b".into(), "c".into()],
                Some(&dir.path().to_string_lossy()),
            )
            .unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let text = loop {
            let t = std::fs::read_to_string(&out).unwrap_or_default();
            if t.lines().count() >= 2 || std::time::Instant::now() > end {
                break t;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let want_dir = dir.path().canonicalize().unwrap();
        let mut lines = text.lines();
        assert_eq!(
            std::path::Path::new(lines.next().unwrap())
                .canonicalize()
                .unwrap(),
            want_dir
        );
        assert_eq!(lines.next(), Some("a b|c"));
    }
}
