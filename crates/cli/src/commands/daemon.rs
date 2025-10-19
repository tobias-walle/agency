use crate::{
  rpc::client,
  util::daemon_proc::{resolve_socket, spawn_daemon_background},
};
use std::path::PathBuf;

pub fn print_status() {
  match resolve_socket() {
    Some(sock) => {
      let res = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap()
        .block_on(async move { client::daemon_status(&sock).await });
      match res {
        Ok(status) => {
          println!(
            "daemon: running (v{}, pid {}, socket {})",
            status.version, status.pid, status.socket_path
          );
        }
        Err(_) => {
          println!("daemon: stopped");
        }
      }
    }
    None => println!("daemon: stopped"),
  }
}

pub fn run_daemon_foreground() {
  let Some(sock) = resolve_socket() else {
    eprintln!("could not resolve socket path");
    std::process::exit(1);
  };
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_io()
    .enable_time()
    .worker_threads(2)
    .build()
    .unwrap();
  rt.block_on(async move {
    match agency_core::daemon::start(&sock).await {
      Ok(handle) => {
        handle.wait().await;
      }
      Err(e) => {
        eprintln!("failed to start daemon: {e}");
        std::process::exit(1);
      }
    }
  });
}

pub fn start_daemon() {
  let Some(sock) = resolve_socket() else {
    println!("daemon: stopped");
    return;
  };
  let already_running = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap()
    .block_on(async { client::daemon_status(&sock).await.is_ok() });
  if already_running {
    print_status();
    return;
  }

  let resume_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
  if spawn_daemon_background(&sock, &resume_root).is_err() {
    println!("daemon: stopped");
    return;
  }

  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap();
  let running = rt.block_on(async {
    for _ in 0..20u8 {
      if client::daemon_status(&sock).await.is_ok() {
        return true;
      }
      tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
  });
  if running {
    print_status();
  } else {
    println!("daemon: stopped");
  }
}

pub fn stop_daemon() {
  let Some(sock) = resolve_socket() else {
    println!("daemon: stopped");
    return;
  };
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap();
  let _ = rt.block_on(async move {
    let _ = client::daemon_shutdown(&sock).await;
    for _ in 0..20u8 {
      if client::daemon_status(&sock).await.is_err() {
        return true;
      }
      tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
  });
  println!("daemon: stopped");
}

pub fn restart_daemon() {
  stop_daemon();
  start_daemon();
}
