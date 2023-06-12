//! sync api for executing external git commands

use anyhow::{anyhow, Result};
use std::{
	os::unix::prelude::PermissionsExt,
	path::PathBuf,
	process::{Command, Stdio},
	time::Duration,
};

use raw_sync::{events::*, Timeout};
use shared_memory::*;

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

	fn set_permissions(&mut self) -> Result<()> {
		let mut perm = std::fs::metadata(self.cache_path.as_path())?
			.permissions();
		perm.set_mode(0o777);
		std::fs::set_permissions(self.cache_path.as_path(), perm)?;
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
		if let Err(e) =
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

impl IPCEvents {
	const SHMEM_SIZE: usize = 4096;
	///create shared memory and events on the main side
	pub fn main(event_id: &str) -> Result<Self> {
		let shmem = ShmemConf::new()
			.size(Self::SHMEM_SIZE)
			.flink(format!("gitui_{}", event_id))
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
			.size(4096)
			.flink(format!("gitui_{}", event_id))
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

	///obtain a copy of the string written right after 2 event objects in the shared memory
	pub fn get_str(&self) -> String {
		unsafe {
			let str_len =
				self.shared_mem
					.as_ptr()
					.offset(self.str_offset as isize) as *const usize;
			let str_bytes = &self.shared_mem.as_slice()[self
				.str_offset
				+ std::mem::size_of::<usize>()
				..*str_len];
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
			let str_bytes = &mut self.shared_mem.as_slice_mut()[self
				.str_offset
				+ std::mem::size_of::<usize>()
				..*str_len];
			str_bytes.clone_from_slice(s.as_bytes());
		}
		Ok(())
	}
}

///
pub fn rebase_interactive(repo: &str, base: &str) -> Result<()> {
	let event_id = format!("{}", std::process::id());
	let mut sequence_editor = TempEditor::new(event_id.as_str());
	sequence_editor.create()?;

	let mut cmd = Command::new("git");
	cmd.current_dir(repo)
		.arg("-c")
		.arg(format!(
			"sequence.editor={}",
			sequence_editor.to_str().unwrap(),
		))
		.arg("rebase")
		.arg("-i")
		.arg(base)
		.stdout(Stdio::null()) //muting output. TODO: redirect?
		.stderr(Stdio::null());

	let events = IPCEvents::main(&event_id)?;
	let mut child = cmd.spawn()?;
	events
		.connected_ready
		.wait(Timeout::Val(Duration::from_millis(5000)))
		.map_err(|e| {
			anyhow!("Waiting for a sequence editor to start failed with {}", e)
		})?;
	let todo_file = events.get_str();
	//TODO: here goes the usage of machinery with shared memory, events, and so on...

	//TODO: somewhere here we're ready and need to tell the editor to shutdown
	events
		.connected_shutdown
		.set(EventState::Signaled)
		.map_err(|e| {
			anyhow!(
				"Signaling to editor to shutdown failed with {}",
				e
			)
		})?;
	child.wait()?;
	Ok(())
}
