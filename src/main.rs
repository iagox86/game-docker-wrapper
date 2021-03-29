use tokio::{io, task, process};
use tokio::sync::Mutex;
use tokio::prelude::*;
use tokio::signal::unix::{signal, SignalKind};
use std::process::{Stdio, exit};
use std::sync::Arc;
use clap::{App, Arg};
use tokio::io::AsyncBufReadExt;

async fn input_task(child_stdin: Arc<Mutex<process::ChildStdin>>) {
  let mut lines = io::BufReader::new(io::stdin()).lines();

  loop {
    let line = lines.next_line().await;

    let line = match line {
        Ok(l) => l,
        Err(e) => {
            eprintln!("WARNING: can't read from stdin: {}", e);
            return;
        },
    };

    let mut line = match line {
        Some(l) => l,
        None => {
            eprintln!("WARNING: can't read from stdin: closed");
            return;
        },
    };
    line.push('\n');

    child_stdin.lock().await.write_all(&line.into_bytes()).await.unwrap_or_else(|e| {
      eprintln!("Unable to write to child process: {}", e);
      exit(1);
    });
  }
}

#[tokio::main]
async fn main() {
  let matches = App::new("game-docker-wrapper")
    .author("Ron <ron@skullsecurity.net>")
    .version("1.0")
    .about("A wrapper for dockerizing game servers. Runs a given binary, catches SIGTERM (the signal used by Docker to terminate containers), and sends an exit command.")

    // Debug
    .arg(Arg::with_name("debug")
      .short("d")
      .long("debug")
      .help("Debug output")
      .takes_value(false)
    )

    // Kill command
    .arg(Arg::with_name("kill-command")
      .short("k")
      .long("kill-command")
      .help("The command to send on stdin of the wrapped process after SIGTERM")
      .takes_value(true)
    )

    // Kill options
    .arg(Arg::with_name("no-newline-before-kill")
      .short("n")
      .long("no-newline-before")
      .help("Don't send a newline (\\n) before the kill-command")
      .takes_value(false)
      .requires("kill-command")
    )

    // Kill options
    .arg(Arg::with_name("no-newline-after-kill")
      .short("N")
      .long("no-newline-after")
      .help("Don't send a newline (\\n) after the kill-command")
      .takes_value(false)
      .requires("kill-command")
    )

    // The actual command
    .arg(Arg::with_name("binary + params")
      .multiple(true)
      .last(true)
      .required(true)
    )

    // Done
    .get_matches();

  // Get the commandline arguments
  let debug = matches.is_present("debug");
  let kill_command = matches.value_of("kill-command");
  let newline_before_kill = !matches.is_present("no-newline-before-kill");
  let newline_after_kill = !matches.is_present("no-newline-after-kill");

  // Pull out the binary and parameters as an iterator (ignore errors, since the
  // library handles them)
  let mut binary_args = matches.values_of("binary + params").unwrap();

  // Get the binary, or bail - I don't *think* this can fail, but better to be
  // safe
  let binary = binary_args.next().unwrap_or_else(|| {
    eprintln!("Missing binary argument - must be after a '--' mark");
    exit(1);
  });

  // Collect up the arguments, if any
  let binary_args: Vec<&str> = binary_args.collect();

  if debug {
    eprintln!("Running command: {}", binary)
  }

  // Spawn a child process
  let mut child = process::Command::new(binary).args(binary_args).stdin(Stdio::piped()).spawn().unwrap_or_else(|e| {
    eprintln!("Error creating process: {}", e);
    exit(1);
  });

  // Get the child's stdin
  let child_stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));

  // Create a task that feeds the child stdin from our stdin
  task::spawn(input_task(child_stdin.clone()));

  // Wait for a terminate signal
  signal(SignalKind::terminate()).expect("stream error").recv().await;
  if debug {
    match kill_command {
      Some(kill_command) => eprintln!("SIGTERM received! Sending kill command to the child: {}", kill_command),
      None => eprintln!("SIGTERM received! Performing a clean shutdown"),
    }
  }

  // Grab a lock on the child_stdin process (and don't ever release it)
  let mut child_stdin = child_stdin.lock().await;

  // Optionally write the newlines and kill-command
  if newline_before_kill {
    child_stdin.write_all("\n".as_bytes()).await.unwrap_or_else(|e| {
      eprintln!("Error writing kill command to child: {}", e);
      exit(1);
    });
  }
  if let Some(kill_command) = kill_command {
    child_stdin.write_all(kill_command.as_bytes()).await.unwrap_or_else(|e| {
      eprintln!("Error writing kill command to child: {}", e);
      exit(1);
    });
  }
  if newline_after_kill {
    child_stdin.write_all("\n".as_bytes()).await.unwrap_or_else(|e| {
      eprintln!("Error writing kill command to child: {}", e);
      exit(1);
    });
  }

  if debug {
    eprintln!("Waiting for child process to exit...");
  }

  // Wait for child to exit
  let status = child.await;
  if debug {
    match status {
      Ok(status) => eprintln!("Child process ended with status: {}", status),
      Err(e)     => eprintln!("An error occurred while the child was exiting: {}", e),
    };
  }

  // Stop the process cleanly (otherwise, we'll be waiting forever on the stdin
  // thread)
  exit(0);
}
