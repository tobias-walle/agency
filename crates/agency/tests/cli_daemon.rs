mod common;

use anyhow::Result;
use predicates::prelude::*;
use crate::common::test_env::TestEnv;

#[test]
fn ps_autostarts_daemon_when_missing() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    if !env.sockets_available() {
      eprintln!(
        "Skipping ps_autostarts_daemon_when_missing: Unix sockets not available in sandbox"
      );
      return Ok(());
    }

    env.with_env_vars(&[("AGENCY_NO_AUTOSTART", None)], |env| -> Result<()> {
      env
        .agency()?
        .arg("tasks")
        .assert()
        .success()
        .stdout(predicates::str::contains("ID SLUG").from_utf8());
      Ok(())
    })?;

    env.agency_daemon_stop()?;

    Ok(())
  })
}

#[test]
fn daemon_reports_version_via_protocol() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    if !env.sockets_available() {
      eprintln!(
        "Skipping daemon_reports_version_via_protocol: Unix sockets not available in sandbox"
      );
      return Ok(());
    }

    env.agency_daemon_start()?;

    let socket = env.runtime_dir().join("agency.sock");
    let mut stream = std::os::unix::net::UnixStream::connect(&socket)?;
    agency::daemon_protocol::write_frame(
      &mut stream,
      &agency::daemon_protocol::C2D::Control(agency::daemon_protocol::C2DControl::GetVersion),
    )?;
    let reply: agency::daemon_protocol::D2C = agency::daemon_protocol::read_frame(&mut stream)?;
    match reply {
      agency::daemon_protocol::D2C::Control(agency::daemon_protocol::D2CControl::Version {
        version,
      }) => {
        let cli_version = env!("CARGO_PKG_VERSION");
        assert_eq!(
          version, cli_version,
          "daemon version must match CLI version",
        );
      }
      other @ agency::daemon_protocol::D2C::Control(_) => {
        panic!("unexpected reply: {other:?}")
      }
    }

    env.agency_daemon_stop()?;

    Ok(())
  })
}

#[test]
fn ps_lists_id_and_slug_in_order() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    if !env.sockets_available() {
      eprintln!("Skipping ps_lists_id_and_slug_in_order: Unix sockets not available in sandbox");
      return Ok(());
    }
    let (id1, slug1) = env.new_task("alpha-task", &[])?;
    let (id2, slug2) = env.new_task("beta-task", &[])?;

    env.agency_daemon_start()?;

    env
      .agency()?
      .arg("tasks")
      .assert()
      .success()
      .stdout(predicates::str::contains("ID SLUG").from_utf8())
      .stdout(
        predicates::str::is_match(r"STATUS +UNCOMMITTED +COMMITS +BASE +AGENT[^\n]*\n")
          .expect("regex")
          .from_utf8(),
      )
      .stdout(
        predicates::str::is_match(format!(r"\b{}\s+{}\b", id1, regex::escape(&slug1)))
          .expect("regex")
          .from_utf8(),
      )
      .stdout(
        predicates::str::is_match(format!(r"\b{}\s+{}\b", id2, regex::escape(&slug2)))
          .expect("regex")
          .from_utf8(),
      )
      .stdout(predicates::str::contains("Draft").from_utf8());

    env.agency_daemon_stop()?;

    Ok(())
  })
}

#[test]
fn ps_handles_empty_state() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    if !env.sockets_available() {
      eprintln!("Skipping ps_handles_empty_state: Unix sockets not available in sandbox");
      return Ok(());
    }

    env.agency_daemon_start()?;

    env
      .agency()?
      .arg("tasks")
      .assert()
      .success()
      .stdout(predicates::str::contains("ID SLUG").from_utf8())
      .stdout(
        predicates::str::is_match(r"STATUS +UNCOMMITTED +COMMITS +BASE +AGENT.*\n")
          .expect("regex")
          .from_utf8(),
      );

    env.agency_daemon_stop()?;

    Ok(())
  })
}

#[test]
fn ps_bails_when_daemon_not_running() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env.agency()?.arg("tasks").assert().success().stdout(
      predicates::str::contains("ID SLUG STATUS UNCOMMITTED COMMITS BASE AGENT").from_utf8(),
    );

    Ok(())
  })
}

#[test]
fn sessions_bails_when_daemon_not_running() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env.agency()?.arg("sessions").assert().failure().stderr(
      predicates::str::contains("Daemon not running. Please start it with `agency daemon start`")
        .from_utf8(),
    );

    Ok(())
  })
}
