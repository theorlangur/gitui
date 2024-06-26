use anyhow::Result;
use asyncgit::sync::{
	diff::DiffOptions, repo_dir, GitExternCommands, RepoPathRef,
	ShowUntrackedFilesConfig,
};
use crossterm::event::KeyEvent;
use ron::{
	de::from_bytes,
	ser::{to_string_pretty, PrettyConfig},
};
use serde::{Deserialize, Serialize};
use std::{
	cell::RefCell,
	fs::File,
	io::{Read, Write},
	path::PathBuf,
	rc::Rc,
};

use crate::keys::key_match;
use crate::keys::GituiKeyEvent;

type ExternCmdList = Vec<(String, Option<GituiKeyEvent>)>;

#[derive(Default, Clone, Serialize, Deserialize)]
struct OptionsData {
	pub tab: usize,
	pub diff: DiffOptions,
	pub status_show_untracked: Option<ShowUntrackedFilesConfig>,
	pub commit_msgs: Vec<String>,
	pub extern_cmds: ExternCmdList,
	pub git_extern_cmds: GitExternCommands,
	pub branch_shortcuts: Vec<(String, GituiKeyEvent)>,
}

const COMMIT_MSG_HISTRY_LENGTH: usize = 20;

#[derive(Clone)]
pub struct Options {
	repo: RepoPathRef,
	data: OptionsData,
}

pub type SharedOptions = Rc<RefCell<Options>>;

impl Options {
	pub fn new(repo: RepoPathRef) -> SharedOptions {
		Rc::new(RefCell::new(Self {
			data: Self::read(&repo).unwrap_or_default(),
			repo,
		}))
	}

	pub fn set_current_tab(&mut self, tab: usize) {
		self.data.tab = tab;
		self.save();
	}

	pub const fn current_tab(&self) -> usize {
		self.data.tab
	}

	pub const fn diff_options(&self) -> DiffOptions {
		self.data.diff
	}

	pub const fn status_show_untracked(
		&self,
	) -> Option<ShowUntrackedFilesConfig> {
		self.data.status_show_untracked
	}

	pub fn set_status_show_untracked(
		&mut self,
		value: Option<ShowUntrackedFilesConfig>,
	) {
		self.data.status_show_untracked = value;
		self.save();
	}

	pub fn diff_context_change(&mut self, increase: bool) {
		self.data.diff.context = if increase {
			self.data.diff.context.saturating_add(1)
		} else {
			self.data.diff.context.saturating_sub(1)
		};

		self.save();
	}

	pub fn diff_hunk_lines_change(&mut self, increase: bool) {
		self.data.diff.interhunk_lines = if increase {
			self.data.diff.interhunk_lines.saturating_add(1)
		} else {
			self.data.diff.interhunk_lines.saturating_sub(1)
		};

		self.save();
	}

	pub fn diff_toggle_whitespace(&mut self) {
		self.data.diff.ignore_whitespace =
			!self.data.diff.ignore_whitespace;

		self.save();
	}

	pub fn git_extern_commands(&self) -> &GitExternCommands {
		&self.data.git_extern_cmds
	}

	pub fn set_git_extern_push(&mut self, cmd: Option<String>) {
		self.data.git_extern_cmds.push_base = cmd;
		self.save();
	}

	pub fn set_git_extern_fetch(&mut self, cmd: Option<String>) {
		self.data.git_extern_cmds.fetch_base = cmd;
		self.save();
	}

	pub fn set_git_extern_checkout(&mut self, cmd: Option<String>) {
		self.data.git_extern_cmds.checkout_base = cmd;
		self.save();
	}

	pub fn extern_commands(&self) -> &ExternCmdList {
		&self.data.extern_cmds
	}

	pub fn remove_extern_command(&mut self, idx: usize) -> usize {
		if idx < self.data.extern_cmds.len() {
			self.data.extern_cmds.remove(idx);
			self.save();
			if idx == self.data.extern_cmds.len() {
				idx.saturating_sub(1)
			} else {
				idx
			}
		} else {
			0
		}
	}

	pub fn add_extern_command(&mut self, cmd: &str) {
		let existing = self
			.data
			.extern_cmds
			.iter()
			.enumerate()
			.find(|i| i.1 .0 == cmd);
		if existing.is_none() {
			//add new
			self.data.extern_cmds.insert(0, (cmd.to_string(), None));
			self.save();
		}
	}

	pub fn assign_shortcut_for_extern_command(
		&mut self,
		idx: usize,
		shortcut: Option<GituiKeyEvent>,
	) {
		self.data.extern_cmds[idx].1 = shortcut;
		self.save();
	}

	pub fn clear_all_shortcuts_for_extern_commands(&mut self) {
		self.data.extern_cmds.iter_mut().for_each(|i| i.1 = None);
		self.save();
	}

	pub fn find_extern_cmd_for_shortcut(
		&self,
		e: GituiKeyEvent,
	) -> Option<String> {
		self.data
			.extern_cmds
			.iter()
			.find(|i| i.1 == Some(e))
			.map(|i| i.0.clone())
	}

	pub fn assign_shortcut_for_branch(
		&mut self,
		branch: &str,
		e: &KeyEvent,
	) {
		let shortcut = GituiKeyEvent::new(e.code, e.modifiers);
		let existing = self
			.data
			.branch_shortcuts
			.iter_mut()
			.find(|i| i.0 == branch);
		if let Some(i) = existing {
			i.1 = shortcut;
		} else {
			self.data
				.branch_shortcuts
				.push((branch.to_string(), shortcut));
		}
		self.save();
	}

	pub fn remove_shortcut_for_branch(&mut self, branch: &str) {
		self.data.branch_shortcuts.retain(|i| i.0 != branch);
		self.save();
	}

	pub fn clear_all_branch_shortcuts(&mut self) {
		self.data.branch_shortcuts.clear();
		self.save();
	}

	pub fn find_branch_by_key_event(
		&self,
		e: &KeyEvent,
	) -> Option<&str> {
		self.data
			.branch_shortcuts
			.iter()
			.find(|i| key_match(e, i.1))
			.map(|i| i.0.as_str())
	}

	pub fn has_any_branch_shortcuts(&self) -> bool {
		!self.data.branch_shortcuts.is_empty()
	}

	pub fn find_branch_shortcut_by_branch(
		&self,
		branch: &str,
	) -> Option<&GituiKeyEvent> {
		self.data
			.branch_shortcuts
			.iter()
			.find(|i| i.0 == branch)
			.map(|i| &i.1)
	}

	pub fn add_commit_msg(&mut self, msg: &str) {
		self.data.commit_msgs.push(msg.to_owned());
		while self.data.commit_msgs.len() > COMMIT_MSG_HISTRY_LENGTH {
			self.data.commit_msgs.remove(0);
		}
		self.save();
	}

	pub fn has_commit_msg_history(&self) -> bool {
		!self.data.commit_msgs.is_empty()
	}

	pub fn commit_msg(&self, idx: usize) -> Option<String> {
		if self.data.commit_msgs.is_empty() {
			None
		} else {
			let entries = self.data.commit_msgs.len();
			let mut index = idx;

			while index >= entries {
				index -= entries;
			}

			index = entries.saturating_sub(1) - index;

			Some(self.data.commit_msgs[index].to_string())
		}
	}

	fn save(&self) {
		if let Err(e) = self.save_failable() {
			log::error!("options save error: {}", e);
		}
	}

	fn read(repo: &RepoPathRef) -> Result<OptionsData> {
		let dir = Self::options_file(repo)?;

		let mut f = File::open(dir)?;
		let mut buffer = Vec::new();
		f.read_to_end(&mut buffer)?;
		Ok(from_bytes(&buffer)?)
	}

	//TODO: fix once FP in clippy is fixed
	#[allow(clippy::needless_borrow)]
	fn save_failable(&self) -> Result<()> {
		let dir = Self::options_file(&self.repo)?;

		let mut file = File::create(&dir)?;
		let data =
			to_string_pretty(&self.data, PrettyConfig::default())?;
		file.write_all(data.as_bytes())?;

		Ok(())
	}

	fn options_file(repo: &RepoPathRef) -> Result<PathBuf> {
		let dir = repo_dir(&repo.borrow())?;
		let dir = dir.join("gitui");
		Ok(dir)
	}
}
