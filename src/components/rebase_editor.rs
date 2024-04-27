use anyhow::{anyhow, Result};
use asyncgit::sync::CommitId;
use crossterm::{
	terminal::{EnterAlternateScreen, LeaveAlternateScreen},
	ExecutableCommand,
};
use scopeguard::defer;
use std::{io, process::{Command, Stdio}};

///
pub fn rebase_commits_interactive_with_editor(
	repo: &str,
	base: &CommitId,
) -> Result<()> {
	io::stdout().execute(LeaveAlternateScreen)?;
	defer! {
		io::stdout().execute(EnterAlternateScreen).expect("reset terminal");
	}

	let base_hash = base.to_string();

	let mut cmd = Command::new("git");
	cmd.current_dir(repo)
		.arg("rebase")
		.arg("-i")
		.arg(base_hash)
		.status()
		.map_err(|e| anyhow!("rebase -i: {}", e))?;
	Ok(())
}

///
pub fn rebase_commits_continue_with_editor(
	repo: &str,
) -> Result<()> {
	io::stdout().execute(LeaveAlternateScreen)?;
	defer! {
		io::stdout().execute(EnterAlternateScreen).expect("reset terminal");
	}

	let mut cmd = Command::new("git");
	cmd.current_dir(repo)
		.arg("rebase")
		.arg("--continue")
		.status()
		.map_err(|e| anyhow!("rebase --continue: {}", e))?;
	Ok(())
}

///
pub fn rebase_interactive_abort(
	repo: &str,
) -> Result<()> {
	let mut cmd = Command::new("git");
	let _ = cmd.current_dir(repo)
		.arg("rebase")
		.arg("--abort")
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status();
	Ok(())
}

///
pub fn rebase_interactive_skip(
	repo: &str,
) -> Result<()> {
	let mut cmd = Command::new("git");
	let _ = cmd.current_dir(repo)
		.arg("rebase")
		.arg("--skip")
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status();
	Ok(())
}

