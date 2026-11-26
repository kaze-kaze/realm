#![cfg(all(unix, not(target_os = "android")))]

use std::process::{Command, Output};

#[test]
fn nofile_is_raised_before_reading_file_or_environment_config() {
    fn start(args: &[&str], config: Option<&str>) -> Output {
        let mut child = Command::new("sh");
        child.args(["-c", "ulimit -Sn 16 && exec \"$@\"", "sh", env!("CARGO_BIN_EXE_realm")]);
        child.args(args).env_remove(realm::ENV_CONFIG);
        if let Some(config) = config {
            child.env(realm::ENV_CONFIG, config);
        }
        child.output().unwrap()
    }

    fn limits(output: &Output) -> (u64, u64) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let (soft, hard) = stdout
            .lines()
            .next()
            .unwrap()
            .strip_prefix("fd: soft=")
            .unwrap()
            .split_once(", hard=")
            .unwrap();
        (soft.parse().unwrap(), hard.parse().unwrap())
    }

    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let output = start(&["-c", file], None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to parse"));
    let (soft, hard) = limits(&output);
    assert_eq!(soft, hard);
    assert!(soft > 16);

    let output = start(&["-c", file, "-n", "32"], None);
    assert!(!output.status.success());
    assert_eq!(limits(&output), (32, 32));

    let output = start(&["-c", file, "-n", "32"], Some("endpoints = []"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no endpoints listen on TCP or UDP"));
    assert_eq!(limits(&output), (hard, hard));
}
