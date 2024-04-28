//! sync api for executing external git commands

use anyhow::{anyhow, Result};
use std::{
	collections::HashSet,
	path::PathBuf,
	process::{Command, Stdio},
	time::Duration,
};

use raw_sync::{events::*, Timeout};
use shared_memory::*;

use super::CommitId;

struct TempEditor<'a> {
	cache_path: PathBuf,
	event_id: &'a str,
}

impl<'a> TempEditor<'a> {
	pub fn new(e: &'a str) -> Self {
		Self {
			cache_path: PathBuf::new(),
			event_id: e,
		}
	}

	#[cfg(unix)]
	fn set_permissions(&mut self) -> Result<()> {
		use std::os::unix::prelude::PermissionsExt;
		let mut perm = std::fs::metadata(self.cache_path.as_path())?
			.permissions();
		perm.set_mode(0o777);
		std::fs::set_permissions(self.cache_path.as_path(), perm)?;
		Ok(())
	}

	#[cfg(windows)]
	fn set_permissions(&mut self) -> Result<()> {
		Ok(())
	}

	fn prepare_path(&mut self) -> Result<()> {
		self.cache_path = dirs_next::cache_dir()
			.ok_or_else(|| anyhow!("failed to find os cache dir."))?;

		self.cache_path.push("gitui");
		if let Err(_) = std::fs::create_dir(self.cache_path.as_path())
		{
		}
		Ok(())
	}

	#[cfg(unix)]
	fn create_script(&mut self) -> Result<()> {
		let exe_path = std::env::current_exe()?;
		self.cache_path.push(format!("edit{}.sh", self.event_id));
		std::fs::write(
			self.cache_path.as_os_str(),
			format!(
				"#!/bin/sh\n{} --event_id {} --type rebase \"$@\"",
				exe_path.to_str().unwrap(),
				self.event_id
			),
		)?;
		Ok(())
	}

	#[cfg(windows)]
	fn create_script(&mut self) -> Result<()> {
		let exe_path = std::env::current_exe()?;
		self.cache_path.push(format!("edit{}.bat", self.event_id));
		std::fs::write(
			self.cache_path.as_os_str(),
			format!(
				"{} --event_id {} --type rebase %*",
				exe_path.to_str().unwrap(),
				self.event_id
			),
		)?;
		Ok(())
	}

	pub fn create(&mut self) -> Result<()> {
		self.prepare_path()?;
		self.create_script()?;
		self.set_permissions()?;
		Ok(())
	}

	pub fn to_str(&self) -> Option<&str> {
		self.cache_path.to_str()
	}
}

impl<'a> Drop for TempEditor<'a> {
	fn drop(&mut self) {
		if let Err(_) =
			std::fs::remove_file(self.cache_path.as_path())
		{
			//log somehow
		}
	}
}

///used to create communication 'channel' between gitui and a sequence-editor-gitui instance
pub struct IPCEvents {
	shared_mem: Shmem,
	///an event that sequence-editor-gitui should signal when it's ready
	pub connected_ready: Box<dyn EventImpl>,
	///an event that a main gitui shoud signal when sequence editor should shutdown
	pub connected_shutdown: Box<dyn EventImpl>,

	str_offset: usize,
}
impl Drop for IPCEvents {
	fn drop(&mut self) {
		if let Err(_) = self.signal_connected_shutdown() {}
	}
}

impl IPCEvents {
	const SHMEM_SIZE: usize = 4096;
	///create shared memory and events on the main side
	pub fn main(event_id: &str) -> Result<Self> {
		let shmem = ShmemConf::new()
			.size(Self::SHMEM_SIZE)
			.os_id(format!("gitui_{}", event_id))
			.create()?;
		let ready_res = unsafe { Event::new(shmem.as_ptr(), true) };
		let (e_ready, e_ready_size) = ready_res.map_err(|e| {
			anyhow!("Could not create 'ready' event: {}", e)
		})?;

		let shutdown_res = unsafe {
			Event::new(
				shmem.as_ptr().offset(e_ready_size as isize),
				true,
			)
		};
		let (e_shutdown, e_shutdown_size) =
			shutdown_res.map_err(|e| {
				anyhow!("Could not create 'shutdown' event: {}", e)
			})?;
		Ok(Self {
			shared_mem: shmem,
			connected_ready: e_ready,
			connected_shutdown: e_shutdown,
			str_offset: e_ready_size + e_shutdown_size,
		})
	}

	///connect to already existing shared memory and events
	pub fn connected(event_id: &str) -> Result<Self> {
		let shmem = ShmemConf::new()
			.size(Self::SHMEM_SIZE)
			.os_id(format!("gitui_{}", event_id))
			.open()?;
		let ready_res =
			unsafe { Event::from_existing(shmem.as_ptr()) };
		let (e_ready, e_ready_size) = ready_res.map_err(|e| {
			anyhow!("Could not open 'ready' event: {}", e)
		})?;

		let shutdown_res = unsafe {
			Event::from_existing(
				shmem.as_ptr().offset(e_ready_size as isize),
			)
		};
		let (e_shutdown, e_shutdown_size) =
			shutdown_res.map_err(|e| {
				anyhow!("Could not open 'shutdown' event: {}", e)
			})?;

		Ok(Self {
			shared_mem: shmem,
			connected_ready: e_ready,
			connected_shutdown: e_shutdown,
			str_offset: e_ready_size + e_shutdown_size,
		})
	}

	///wait until connected app is connected and ready
	pub fn wait_connected_ready(&self) -> Result<()> {
		self.connected_ready
			.wait(Timeout::Val(Duration::from_millis(5000)))
			//.wait(Timeout::Infinite)
			.map_err(|e| {
				anyhow!("Waiting for a sequence editor to start failed with {}", e)
			})?;
		Ok(())
	}

	///signal the main app that the connected is ready
	pub fn signal_connected_ready(&self) -> Result<()> {
		self.connected_ready.set(EventState::Signaled).map_err(
			|e| anyhow!("Could not signal 'ready' event: {}", e),
		)?;
		Ok(())
	}

	///signal the other side that it can shutdown
	pub fn signal_connected_shutdown(&self) -> Result<()> {
		self.connected_shutdown.set(EventState::Signaled).map_err(
			|e| {
				anyhow!(
					"Signaling to editor to shutdown failed with {}",
					e
				)
			},
		)?;
		Ok(())
	}

	///wait till connected_shutdown is signaled
	pub fn wait_shutdown(&self) -> Result<()> {
		self.connected_shutdown
			.wait(raw_sync::Timeout::Infinite)
			.map_err(|e| {
				anyhow!("Failed to wait for 'shutdown' event: {}", e)
			})?;
		Ok(())
	}

	///obtain a copy of the string written right after 2 event objects in the shared memory
	pub fn get_str(&self) -> String {
		unsafe {
			let str_len =
				self.shared_mem
					.as_ptr()
					.offset(self.str_offset as isize) as *const usize;
			let str_bytes_from =
				self.str_offset + std::mem::size_of::<usize>();
			let str_bytes_to = str_bytes_from + *str_len;
			let str_bytes = &self.shared_mem.as_slice()
				[str_bytes_from..str_bytes_to];
			std::str::from_utf8(str_bytes).unwrap_or("").to_string()
		}
	}

	///should be called from the connected part to set the string into shared mem
	pub fn set_str(&mut self, s: &str) -> Result<()> {
		if s.len()
			> Self::SHMEM_SIZE
				- self.str_offset
				- std::mem::size_of::<usize>()
		{
			return Err(anyhow!("String is too big"));
		}
		unsafe {
			let str_len =
				self.shared_mem
					.as_ptr()
					.offset(self.str_offset as isize) as *mut usize;
			*str_len = s.len();
			let str_bytes_from =
				self.str_offset + std::mem::size_of::<usize>();
			let str_bytes_to = str_bytes_from + *str_len;
			let str_bytes = &mut self.shared_mem.as_slice_mut()
				[str_bytes_from..str_bytes_to];
			str_bytes.clone_from_slice(s.as_bytes());
		}
		Ok(())
	}
}

struct TerminalState {
	#[cfg(windows)]
	handle: windows::Win32::Foundation::HANDLE,
	#[cfg(windows)]
	mode: windows::Win32::System::Console::CONSOLE_MODE,
}

impl TerminalState {
	#[cfg(windows)]
	pub fn capture() -> Self {
		let mut console_mode =
			windows::Win32::System::Console::CONSOLE_MODE::default();
		let h: windows::Win32::Foundation::HANDLE;
		unsafe {
			h = windows::Win32::System::Console::GetStdHandle(
				windows::Win32::System::Console::STD_OUTPUT_HANDLE,
			)
			.unwrap();
			windows::Win32::System::Console::GetConsoleMode(
				h,
				&mut console_mode,
			);
		}
		Self {
			handle: h,
			mode: console_mode,
		}
	}

	#[cfg(not(windows))]
	pub fn capture() -> Self {
		Self{}
	}
}

#[cfg(windows)]
impl Drop for TerminalState {
	fn drop(&mut self) {
		unsafe {
			windows::Win32::System::Console::SetConsoleMode(
				self.handle,
				self.mode,
			);
		}
	}
}

///
pub fn rebase_interactive<F>(
	repo: &str,
	base: &str,
	f: F,
) -> Result<()>
where
	F: Fn(&str) -> Result<()>,
{
	let event_id = format!("{}", std::process::id());
	let mut sequence_editor = TempEditor::new(event_id.as_str());
	sequence_editor.create()?;

	let mut cmd = Command::new("git");
	cmd.current_dir(repo)
		.arg("-c")
		.arg(format!(
			"sequence.editor='{}'",
			sequence_editor.to_str().unwrap(),
		))
		.arg("-c")
		.arg("rebase.instructionFormat=\"%H\"")
		.arg("rebase")
		.arg("-i")
		.arg(base)
		.stdout(Stdio::null()) //muting output. TODO: redirect?
		.stderr(Stdio::null());

	let terminal_state = TerminalState::capture();
	let events = IPCEvents::main(&event_id)?;
	let mut child = cmd.spawn()?;
	events.wait_connected_ready()?;
	let todo_file = events.get_str();
	f(&todo_file)?;
	events.signal_connected_shutdown()?;
	child.wait()?;
	drop(terminal_state);
	Ok(())
}

///
pub fn rebase_drop_commits(
	repo: &str,
	commits: Vec<&CommitId>,
	base: &CommitId,
) -> Result<()> {
	let hashed_commits = commits
		.iter()
		.map(|i| i.to_string())
		.collect::<HashSet<String>>();
	rebase_interactive(
		repo,
		base.to_string().as_str(),
		|todo_file| {
			let rebase_commits: Vec<_> =
				parse_rebase_todo(todo_file)?
					.into_iter()
					.map(|i| {
						if hashed_commits.contains(&i.full_hash) {
							i.change_op(InteractiveOperation::Drop)
						} else {
							i
						}
					})
					.collect();
			write_rebase_todo(todo_file, rebase_commits)?;
			Ok(())
		},
	)?;
	Ok(())
}

///
pub fn rebase_fixup_commits(
	repo: &str,
	commits: Vec<&CommitId>,
	base: &CommitId,
) -> Result<()> {
	let hashed_commits = commits
		.iter()
		.map(|i| i.to_string())
		.collect::<HashSet<String>>();
	rebase_interactive(
		repo,
		base.to_string().as_str(),
		|todo_file| {
			let rebase_commits: Vec<_> =
				parse_rebase_todo(todo_file)?
					.into_iter()
					.map(|i| {
						if hashed_commits.contains(&i.full_hash) {
							i.change_op(InteractiveOperation::Fixup)
						} else {
							i
						}
					})
					.collect();
			write_rebase_todo(todo_file, rebase_commits)?;
			Ok(())
		},
	)?;
	Ok(())
}

///
pub enum InteractiveOperation {
	///
	Pick,
	///
	Reword,
	///
	Edit,
	///
	Squash,
	///
	Fixup,
	///
	Exec,
	///
	Break,
	///
	Drop,
	///
	Label,
	///
	Reset,
	///
	Merge,
	///
	UpdateRef,
}

impl InteractiveOperation {
	///
	pub fn try_parse(w: &str) -> Result<InteractiveOperation> {
		match w {
			"pick" | "p" => Ok(InteractiveOperation::Pick),
			"reword" | "r" => Ok(InteractiveOperation::Reword),
			"edit" | "e" => Ok(InteractiveOperation::Edit),
			"squash" | "s" => Ok(InteractiveOperation::Squash),
			"fixup" | "f" => Ok(InteractiveOperation::Fixup),
			"exec" | "x" => Ok(InteractiveOperation::Exec),
			"break" | "b" => Ok(InteractiveOperation::Break),
			"drop" | "d" => Ok(InteractiveOperation::Drop),
			"label" | "l" => Ok(InteractiveOperation::Label),
			"reset" | "t" => Ok(InteractiveOperation::Reset),
			"merge" | "m" => Ok(InteractiveOperation::Merge),
			"update-ref" | "u" => Ok(InteractiveOperation::UpdateRef),
			_ => Err(anyhow!("Unknown operation: {}", w)),
		}
	}

	///
	pub fn to_string(&self) -> String {
		match self {
			InteractiveOperation::Pick => "pick".to_owned(),
			InteractiveOperation::Reword => "reword".to_owned(),
			InteractiveOperation::Edit => "edit".to_owned(),
			InteractiveOperation::Squash => "squash".to_owned(),
			InteractiveOperation::Fixup => "fixup".to_owned(),
			InteractiveOperation::Exec => "exec".to_owned(),
			InteractiveOperation::Break => "break".to_owned(),
			InteractiveOperation::Drop => "drop".to_owned(),
			InteractiveOperation::Label => "label".to_owned(),
			InteractiveOperation::Reset => "reset".to_owned(),
			InteractiveOperation::Merge => "merge".to_owned(),
			InteractiveOperation::UpdateRef => {
				"update-ref".to_owned()
			}
		}
	}
}

///
pub struct RebaseCommit {
	op: InteractiveOperation,
	hash: String,
	full_hash: String,
}

impl RebaseCommit {
	///
	pub fn change_op(self, op: InteractiveOperation) -> Self {
		Self {
			op,
			hash: self.hash,
			full_hash: self.full_hash,
		}
	}

	///
	pub fn try_parse(l: &str) -> Result<RebaseCommit> {
		let mut i = l.split_ascii_whitespace();
		let op = if let Some(w) = i.next() {
			InteractiveOperation::try_parse(w)
		} else {
			Err(anyhow!("no op"))
		}?;
		let hash = if let Some(h) = i.next() {
			Ok(h.to_string())
		} else {
			Err(anyhow!("no short hash"))
		}?;
		let full_hash = if let Some(h) = i.next() {
			Ok(h.trim_matches('"').to_string())
		} else {
			Err(anyhow!("no full hash"))
		}?;

		Ok(RebaseCommit {
			op,
			hash,
			full_hash,
		})
	}

	///
	pub fn to_string(&self) -> String {
		format!(
			"{} {} \"{}\"",
			self.op.to_string(),
			self.hash,
			self.full_hash
		)
	}
}

///
pub fn parse_rebase_todo(f: &str) -> Result<Vec<RebaseCommit>> {
	let r: Vec<_> = std::fs::read_to_string(f)?
		.lines()
		.filter_map(|i| RebaseCommit::try_parse(i).ok())
		.collect();
	Ok(r)
}

///
pub fn write_rebase_todo(
	f: &str,
	commits: Vec<RebaseCommit>,
) -> Result<()> {
	std::fs::write(
		f,
		commits
			.iter()
			.map(|i| i.to_string())
			.collect::<Vec<_>>()
			.join("\n"),
	)?;
	Ok(())
}
