use std::process::Command;

#[test]
fn cli_version_flag_prints_name_and_version() {
    let bin = env!("CARGO_BIN_EXE_ematix-probe");
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .expect("failed to spawn ematix-probe binary");

    assert!(
        output.status.success(),
        "binary exited non-zero: {:?}",
        output
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout was not utf-8");
    let expected = format!("ematix-probe {}", env!("CARGO_PKG_VERSION"));
    assert!(
        stdout.starts_with(&expected),
        "unexpected --version output: {stdout:?} (expected prefix {expected:?})"
    );
}
