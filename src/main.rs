use tokio::{io, task, process, sync};
use tokio::prelude::*;
use tokio::signal::unix::{signal, SignalKind};
use std::process::{Stdio};
use std::sync::{Arc};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
  let args = vec!["-port", "7777", "-config", "/terraria-server/serverconfig.txt", "-modpath", "/mods", "-tmlsavedirectory", "/terraria"];
  let mut cmd = process::Command::new("/terraria-server/tModLoaderServer");
  //let mut cmd = process::Command::new("src/test");
  let mut child = cmd.args(args).stdin(Stdio::piped()).spawn().expect("failed to spawn command");

  // Get the child's stdin
  let s = Arc::new(sync::Mutex::new(child.stdin.take().unwrap()));

  // Pipe this program's stdin to the other program's stdin
  let s2 = s.clone();
  task::spawn(async move {
    loop {
      let mut stdin = io::stdin();
      let mut buffer = [0; 1];
      println!("Reading...");
      stdin.read(&mut buffer).await.expect("oops");
      println!("Read {:?}", buffer);

      s2.lock().await.write_all(&buffer).await.expect("Write error");
    }
  });

  let mut log = tokio::fs::File::create("/terraria/log.txt").await.unwrap();

  // Wait for a terminate signal
  log.write_all("Waiting for sigterm...\n".as_bytes()).await.unwrap();
  let mut term = signal(SignalKind::terminate()).expect("stream error");
  term.recv().await;
  println!("sigterm!");

  // Take a lock forever
  let mut s = s.lock().await;

  // Take over stdin
  log.write_all("Sigterm received! Sending newline...\n".as_bytes()).await.unwrap();
  s.write_all("\n".as_bytes()).await.expect("Write error");
  log.write_all("Sending exit...\n".as_bytes()).await.unwrap();
  s.write_all("exit\n\n".as_bytes()).await.expect("Write error");
  println!("Written! Waiting for child to exit...");

  log.write_all("Waiting for child...\n".as_bytes()).await.unwrap();
  // Wait for child to exit
  let status = child.await.expect("child process encountered an error");
  println!("Child process ended with status: {}", status);
  log.write_all(format!("Child exited with status {}...\n", status).as_bytes()).await.unwrap();
  std::process::exit(0);
}
