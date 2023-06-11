//! sync api for executing external git commands

use anyhow::{anyhow, Result};
use std::{
	os::unix::prelude::PermissionsExt,
	process::{Command, Stdio},
};

///
pub fn rebase_interactive(repo: &str, base: &str) -> Result<()> {
	let mut cache_path = dirs_next::cache_dir()
		.ok_or_else(|| anyhow!("failed to find os cache dir."))?;
	let exe_path = std::env::current_exe()?;
	let event_id = format!("{}", std::process::id());
	cache_path.push("gitui");
	if let Err(_) = std::fs::create_dir(cache_path.clone()) {}
	cache_path.push(format!("edit{}.sh", event_id));
	std::fs::write(
		cache_path.as_os_str(),
		format!(
			"#!/bin/sh\n{} --event_id {} --type rebase \"$@\"",
			exe_path.to_str().unwrap(),
			event_id
		),
	)?;
	let mut perm =
		std::fs::metadata(cache_path.clone())?.permissions();
	perm.set_mode(0o777);
	std::fs::set_permissions(cache_path.clone(), perm)?;
	let mut cmd = Command::new("git");
	cmd.current_dir(repo)
		.arg("-c")
		.arg(format!(
			"sequence.editor={}",
			cache_path.to_str().unwrap(),
		))
		.arg("rebase")
		.arg("-i")
		.arg(base)
		.stdout(Stdio::null()) //muting output. TODO: redirect?
		.stderr(Stdio::null());
	//TODO: here we need to PREPARE machinery with shared memory, events, and so on...
	let mut child = cmd.spawn()?;
	//TODO: here goes the usage of machinery with shared memory, events, and so on...
	child.wait()?;
	std::fs::remove_file(cache_path)?;
	Ok(())
}
