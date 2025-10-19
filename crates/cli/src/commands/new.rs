use crate::{
  args,
  rpc::client,
  util::{
    base_branch::resolve_base_branch_default, daemon_proc::ensure_daemon_running,
    editor::edit_text, errors::render_rpc_failure,
  },
};
use std::io::IsTerminal;
use tracing::debug;

fn agent_arg_to_core(a: args::AgentArg) -> agency_core::domain::task::Agent {
  match a {
    args::AgentArg::Opencode => agency_core::domain::task::Agent::Opencode,
    args::AgentArg::ClaudeCode => agency_core::domain::task::Agent::ClaudeCode,
    args::AgentArg::Fake => agency_core::domain::task::Agent::Fake,
  }
}

fn agent_opt_to_core(a: Option<args::AgentArg>) -> Option<agency_core::domain::task::Agent> {
  a.map(agent_arg_to_core)
}

pub fn new_task(a: args::NewArgs) {
  let sock = ensure_daemon_running();
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let base_branch = resolve_base_branch_default(&root, &a.base_branch);

  let cfg = agency_core::config::load(Some(&root)).unwrap_or_default();
  let resolved_agent = agent_opt_to_core(a.agent).or(cfg.default_agent);
  if resolved_agent.is_none() {
    eprintln!("new failed: no agent specified. Provide --agent or set default_agent in config.");
    std::process::exit(2);
  }
  let agent = resolved_agent.unwrap();
  debug!(event = "cli_agent_resolved", agent = ?agent, "resolved agent for new task");

  let mut body_opt = a.message.clone();
  if body_opt.is_none() && std::io::stdout().is_terminal() {
    match edit_text("") {
      Ok(s) => body_opt = Some(s),
      Err(e) => {
        eprintln!("failed to capture description via editor: {}", e);
      }
    }
  }
  if let Some(ref s) = body_opt {
    debug!(
      event = "cli_task_body_ready",
      len = s.len(),
      "message provided"
    );
  }

  let params = agency_core::rpc::TaskNewParams {
    project_root: root.display().to_string(),
    slug: a.slug,
    base_branch,
    labels: a.labels,
    agent: agent.clone(),
    body: body_opt.clone(),
  };
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { client::task_new(&sock, params).await });
  match res {
    Ok(info) => {
      if a.draft {
        println!("{} {} draft", info.id, info.slug);
        return;
      }
      let tref = agency_core::rpc::TaskRef {
        id: Some(info.id),
        slug: None,
      };
      let start_res = rt.block_on(async { client::task_start(&sock, &root, tref).await });
      match start_res {
        Ok(sr) => {
          println!("{} {} {:?}", sr.id, sr.slug, sr.status);
          if a.no_attach {
            debug!(
              event = "cli_new_autostart_attach",
              attach = false,
              reason = "flag_no_attach",
              "skipping auto-attach by flag"
            );
            return;
          }
          if std::io::stdout().is_terminal() {
            debug!(
              event = "cli_new_autostart_attach",
              attach = true,
              reason = "stdout_tty",
              "auto-attach for new task"
            );
            let attach_args = crate::args::AttachArgs {
              task: sr.id.to_string(),
              no_replay: false,
            };
            crate::commands::attach::attach_interactive(attach_args);
          } else {
            debug!(
              event = "cli_new_autostart_attach",
              attach = false,
              reason = "stdout_not_tty",
              "stdout not a TTY; skipping auto-attach"
            );
          }
        }
        Err(e) => {
          eprintln!("{}", render_rpc_failure("start", &sock, &e));
          std::process::exit(1);
        }
      }
    }
    Err(e) => {
      eprintln!("{}", render_rpc_failure("new", &sock, &e));
      std::process::exit(1);
    }
  }
}
