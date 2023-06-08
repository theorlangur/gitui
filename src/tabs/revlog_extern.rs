use crate::{
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState,
	},
	keys::SharedKeyConfig,
	queue::Queue,
	ui::style::SharedTheme,
};
use anyhow::Result;
use asyncgit::{
	sync::{CommitId, RepoPathRef},
	AsyncGitNotification,
};
use crossbeam_channel::Sender;
use crossterm::event::Event;
use ratatui::{backend::Backend, layout::Rect, Frame};

use super::Revlog;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
	MainLog,
	CompareLog,
}

///
pub struct RevlogExtern {
	repo: RepoPathRef,
	main_log: Revlog,
	compare_log: Revlog,
	queue: Queue,
	visible: bool,
	key_config: SharedKeyConfig,
	focused: Focus,
}

impl RevlogExtern {
	///
	pub fn new(
		repo: &RepoPathRef,
		queue: &Queue,
		sender: &Sender<AsyncGitNotification>,
		theme: SharedTheme,
		key_config: SharedKeyConfig,
	) -> Self {
		Self {
			repo: repo.clone(),
			queue: queue.clone(),
			main_log: Revlog::new(
				repo,
				queue,
				sender,
				theme.clone(),
				key_config.clone(),
			),
			compare_log: Revlog::new(
				repo,
				queue,
				sender,
				theme.clone(),
				key_config.clone(),
			),
			visible: false,
			key_config,
			focused: Focus::MainLog,
		}
	}

	///
	pub fn select_commit(&mut self, id: CommitId) -> Result<()> {
		self.main_log.select_commit(id)
	}

	///
	pub fn any_work_pending(&self) -> bool {
		self.main_log.any_work_pending()
			|| self.compare_log.any_work_pending()
	}

	///
	pub fn update(&mut self) -> Result<()> {
		if self.is_visible() {
			self.main_log.update()?;
			self.compare_log.update()?;
		}

		Ok(())
	}

	///
	pub fn update_git(
		&mut self,
		ev: AsyncGitNotification,
	) -> Result<()> {
		if self.visible {
			self.main_log.update_git(ev)?;
			self.compare_log.update_git(ev)?;
		}

		Ok(())
	}
}

impl DrawableComponent for RevlogExtern {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		self.main_log.draw(f, area)?;
		Ok(())
	}
}

impl Component for RevlogExtern {
	//TODO: cleanup
	#[allow(clippy::too_many_lines)]
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.visible {
			return self.main_log.event(ev);
		}

		Ok(EventState::NotConsumed)
	}

	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		self.main_log.commands(out, force_all);
		visibility_blocking(self)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
		self.main_log.hide();
		self.compare_log.hide();
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		self.main_log.show()?;
		self.compare_log.show()?;

		self.update()?;

		Ok(())
	}
}
