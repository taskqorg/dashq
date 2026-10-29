//! Host tool requirements (D2 `dashq doctor`): each external tool dashq runs on a host, its minimum version and fix.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Req {
    pub tool: &'static str,
    pub min: &'static str,
    pub need: &'static str,
    pub fix: &'static str,
}

/// `--proto` (HTTPS-only reads) first shipped in curl 7.20.2; every other flag dashq and coreq pass is older.
pub const CURL: Req = Req {
    tool: "curl",
    min: "7.20.2",
    need: "Langfuse, GitHub, enrollment forwarding and update downloads",
    fix: "install curl 7.20.2 or newer from the OS packages (apt-get install curl, brew install curl) and put it on the service PATH",
};

/// taskq v4.6.32: `status --json` admission codes (D23); v4.6.30 answer and control commands (D13).
pub const TASKQ: Req = Req {
    tool: "taskq",
    min: "4.6.32",
    need: "config taskq.binary: board snapshots, answers and task controls",
    fix: "run `taskq update` on this host, or point config taskq.binary at taskq 4.6.32 or newer",
};

/// Chrome 59 brought `--headless --screenshot`.
pub const CHROME: Req = Req {
    tool: "chrome",
    min: "59",
    need: "dashq --snapshot only",
    fix: "install Google Chrome 59 or newer, or set DASHQ_CHROME to one",
};

/// One line per requirement and whether it failed. `taskq`: the configured binary; `chrome`: the renderer found.
pub fn check(curl: &Path, taskq: Option<&Path>, chrome: Option<&Path>) -> Vec<(bool, String)> {
    let mut out = vec![verdict(&CURL, run(curl, &["--version"]).and_then(|o| version(&o).ok_or(format!("no version in `{} --version`", curl.display()))))];
    out.push(match taskq {
        Some(p) => verdict(&TASKQ, taskq_version(p)),
        None => (false, format!("taskq: skipped, taskq.binary not configured (minimum {})", TASKQ.min)),
    });
    out.push(match chrome {
        None => (false, format!("chrome: skipped, not found ({}; minimum {})", CHROME.need, CHROME.min)),
        // chrome.exe --version opens a browser instead of printing a version
        Some(p) if cfg!(windows) => (false, format!("chrome: {} found, version not checked on Windows ({})", p.display(), CHROME.need)),
        Some(p) => verdict(&CHROME, run(p, &["--version"]).and_then(|o| version(&o).ok_or(format!("no version in `{} --version`", p.display())))),
    });
    out
}

/// Failed lines only, for install, update and service start (D2): each names the tool, found version, minimum and fix.
pub fn failures(lines: &[(bool, String)]) -> Vec<&str> {
    lines.iter().filter(|(f, _)| *f).map(|(_, l)| l.as_str()).collect()
}

/// The doctor of a release just installed by `update`: its own requirement table, not this binary's.
/// A failed run without a FAILED line (no `doctor` command, crash) still fails, naming command, status and error.
pub fn release(exe: &Path) -> Vec<(bool, String)> {
    let cmd = format!("{} doctor", exe.display());
    let o = match coreq::quiet(Command::new(exe).arg("doctor")).stdin(Stdio::null()).output() {
        Ok(o) => o,
        Err(e) => return vec![(true, format!("doctor: FAILED `{cmd}`: {e}"))],
    };
    let mut lines: Vec<(bool, String)> = String::from_utf8_lossy(&o.stdout).lines().map(|l| (l.contains(": FAILED "), l.to_string())).collect();
    if !o.status.success() && failures(&lines).is_empty() {
        lines.push((true, format!("doctor: FAILED `{cmd}`: {}: {}", o.status, first_line(&String::from_utf8_lossy(&o.stderr)))));
    }
    lines
}

fn verdict(r: &Req, found: Result<String, String>) -> (bool, String) {
    match found {
        Ok(v) if !coreq::update::newer(r.min, &v) => (false, format!("{} {v}: ok (minimum {}; {})", r.tool, r.min, r.need)),
        Ok(v) => (true, format!("{}: FAILED found {v}, minimum {} ({}); fix: {}", r.tool, r.min, r.need, r.fix)),
        Err(e) => (true, format!("{}: FAILED found none ({e}), minimum {} ({}); fix: {}", r.tool, r.min, r.need, r.fix)),
    }
}

/// The first `N.N…` token of the first line: `curl 8.9.1 (…)`, `Google Chrome 120.0.6099.71`.
fn version(text: &str) -> Option<String> {
    let v = text.lines().next()?.split_whitespace().find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
    let v: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    Some(v.trim_end_matches('.').to_string()).filter(|v| !v.is_empty())
}

/// taskq prints no version offline; its coreq install layout names it: `<install>/releases/<v>/taskq` or the
/// launcher `<install>/taskq` beside `current`.
fn taskq_version(bin: &Path) -> Result<String, String> {
    if !bin.is_file() {
        return Err(format!("{} not found", bin.display()));
    }
    let dir = bin.parent().ok_or("no parent folder")?;
    let v = if dir.parent().and_then(Path::file_name).is_some_and(|n| n == "releases") {
        dir.file_name().map(|n| n.to_string_lossy().into_owned())
    } else {
        std::fs::read_to_string(dir.join("current")).ok().map(|s| s.trim().to_string())
    };
    v.filter(|v| v.starts_with(|c: char| c.is_ascii_digit()) && v.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-'))
        .ok_or(format!("version unknown: {} is neither releases/<version>/ nor beside `current`", bin.display()))
}

/// Stdout of `bin args` within 10 s; a spawn failure, timeout or non-zero exit names the command and its error.
fn run(bin: &Path, args: &[&str]) -> Result<String, String> {
    let cmd = format!("{} {}", bin.display(), args.join(" "));
    let mut child = coreq::quiet(Command::new(bin).args(args))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("`{cmd}`: {e}"))?;
    let start = Instant::now();
    // ponytail: version output is a few lines, so the pipes never fill before exit; read in threads if a tool grows chatty
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| format!("`{cmd}`: {e}"))? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("`{cmd}`: timeout after 10 s"));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let o = child.wait_with_output().map_err(|e| format!("`{cmd}`: {e}"))?;
    if !status.success() {
        return Err(format!("`{cmd}`: {status}: {}", first_line(&String::from_utf8_lossy(&o.stderr))));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// A tool's error for the service log: its first line, at most 200 characters.
pub fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no error output").chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake tool: a script printing `out` and exiting with `code`.
    fn fake(dir: &Path, name: &str, out: &str, code: i32) -> std::path::PathBuf {
        #[cfg(windows)]
        {
            let p = dir.join(format!("{name}.cmd"));
            std::fs::write(&p, format!("@echo off\r\necho {out}\r\nexit /b {code}\r\n")).unwrap();
            p
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\necho '{out}'\nexit {code}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        }
    }

    // defect: pilotq#127 broke silently on an old host tool; doctor must fail naming tool, found, minimum and fix
    #[test]
    fn doctor_fails_on_old_or_missing_tools() {
        let d = std::env::temp_dir().join(format!("dashq-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let old = fake(&d, "curl-old", "curl 7.19.7 (x86_64-pc-linux-gnu) libcurl/7.19.7", 0);
        let new = fake(&d, "curl-new", "curl 8.9.1 (x86_64-pc-linux-gnu) libcurl/8.9.1", 0);
        let broken = fake(&d, "curl-broken", "curl: broken install", 3);

        let lines = check(&old, None, None);
        assert!(lines[0].0 && lines[0].1.contains("found 7.19.7, minimum 7.20.2") && lines[0].1.contains("fix: install curl"), "{}", lines[0].1);
        assert!(!lines[1].0 && !lines[2].0, "unconfigured taskq and absent Chrome are skipped, not failed");
        assert_eq!(failures(&lines).len(), 1);

        let missing = check(&d.join("absent-curl"), None, None);
        assert!(missing[0].0 && missing[0].1.contains("found none") && missing[0].1.contains("absent-curl"), "{}", missing[0].1);
        let exit = check(&broken, None, None);
        assert!(exit[0].0 && exit[0].1.contains("curl-broken"), "a failed run names its command: {}", exit[0].1);
        assert!(!check(&new, None, None)[0].0);

        // taskq: version from the coreq install layout, never by running it
        let rel = |v: &str| {
            let r = d.join("install").join("releases").join(v);
            std::fs::create_dir_all(&r).unwrap();
            std::fs::write(r.join("taskq"), "").unwrap();
            r.join("taskq")
        };
        let t = check(&new, Some(&rel("4.6.20")), None);
        assert!(t[1].0 && t[1].1.contains("found 4.6.20, minimum 4.6.32") && t[1].1.contains("taskq update"), "{}", t[1].1);
        assert!(!check(&new, Some(&rel("4.6.40")), None)[1].0);
        let launcher = d.join("install").join("taskq");
        std::fs::write(&launcher, "").unwrap();
        assert!(check(&new, Some(&launcher), None)[1].0, "no `current`: version unknown fails");
        std::fs::write(d.join("install").join("current"), "4.6.32\n").unwrap();
        assert!(!check(&new, Some(&launcher), None)[1].0, "{}", check(&new, Some(&launcher), None)[1].1);
        assert!(check(&new, Some(&d.join("gone")), None)[1].1.contains("found none"));

        if !cfg!(windows) {
            let chrome = fake(&d, "chrome-old", "Chromium 58.0.3029.110", 0);
            assert!(check(&new, None, Some(&chrome))[2].1.contains("found 58.0.3029.110, minimum 59"));
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    // defect: update read only the new release's stdout, so a doctor that failed without a FAILED line reported nothing
    #[test]
    fn release_doctor_fails_when_its_run_fails() {
        let d = std::env::temp_dir().join(format!("dashq-release-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let silent = release(&fake(&d, "dashq-old", "usage: dashq serve", 2));
        let f = failures(&silent);
        assert!(f.len() == 1 && f[0].contains("dashq-old") && f[0].contains(" doctor`") && f[0].contains('2'), "{silent:?}");
        let listed = release(&fake(&d, "dashq-new", "curl: FAILED found 7.19.7, minimum 7.20.2", 1));
        assert_eq!(failures(&listed), ["curl: FAILED found 7.19.7, minimum 7.20.2"], "a listed failure needs no extra line");
        assert!(failures(&release(&fake(&d, "dashq-ok", "curl 8.9.1: ok", 0))).is_empty());
        assert!(release(&d.join("absent-dashq"))[0].1.contains("absent-dashq doctor`"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn versions_parse() {
        assert_eq!(version("curl 8.9.1 (Windows) libcurl/8.9.1").as_deref(), Some("8.9.1"));
        assert_eq!(version("Google Chrome 120.0.6099.71 \n").as_deref(), Some("120.0.6099.71"));
        assert_eq!(version("curl 7.20.2-DEV").as_deref(), Some("7.20.2"));
        assert_eq!(version("no version here"), None);
        assert!(!coreq::update::newer(CURL.min, "7.20.2") && coreq::update::newer(CURL.min, "7.9.8"));
    }
}
