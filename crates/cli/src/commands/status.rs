use crate::rpc::client;
use crate::util::daemon_proc::ensure_daemon_running;

pub fn list_status() {
  let sock = ensure_daemon_running();
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { client::task_status(&sock, &root).await });
  match res {
    Ok(list) => {
      println!("ID   SLUG                 STATUS");
      for t in list.tasks {
        let status = match t.status {
          agency_core::domain::task::Status::Draft => "draft",
          agency_core::domain::task::Status::Running => "running",
          agency_core::domain::task::Status::Stopped => "stopped",
          agency_core::domain::task::Status::Idle => "idle",
          agency_core::domain::task::Status::Completed => "completed",
          agency_core::domain::task::Status::Reviewed => "reviewed",
          agency_core::domain::task::Status::Failed => "failed",
          agency_core::domain::task::Status::Merged => "merged",
        };
        println!("{:<4} {:<20} {:<10}", t.id, t.slug, status);
      }
    }
    Err(e) => {
      eprintln!(
        "{}",
        crate::util::errors::render_rpc_failure("status", &sock, &e)
      );
      std::process::exit(1);
    }
  }
}
