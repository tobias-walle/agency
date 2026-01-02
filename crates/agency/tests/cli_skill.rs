mod common;

use crate::common::test_env::TestEnv;

#[test]
fn cli_skill_install_requires_tty() {
  TestEnv::run(|env| {
    let mut cmd = env.agency().expect("agency cmd");
    cmd.arg("skill").arg("install");
    let assert = cmd.assert().failure();
    let output = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
      output.contains("This command requires an interactive terminal (TTY)"),
      "expected TTY error, got: {output}"
    );
  });
}
