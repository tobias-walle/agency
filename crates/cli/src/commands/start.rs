use crate::{
  args,
  rpc::client,
  util::{
    daemon_proc::ensure_daemon_running, errors::render_rpc_failure, task_ref::parse_task_ref,
  },
};

pub fn start_task(a: args::StartArgs) {
  let sock = ensure_daemon_running();
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let tref = parse_task_ref(&a.task);
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { client::task_start(&sock, &root, tref).await });
  match res {
    Ok(r) => {
      println!("{} {} {:?}", r.id, r.slug, r.status);
    }
    Err(e) => {
      eprintln!("{}", render_rpc_failure("start", &sock, &e));
      std::process::exit(1);
    }
  }
}
