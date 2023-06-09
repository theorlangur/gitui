use crate::{
	components::{
		visibility_blocking, CommandBlocking, CommandInfo, Component,
		DrawableComponent, EventState,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{InternalEvent, LocalEvent, Queue, SharedLocalQueue},
	ui::style::SharedTheme,
};
use anyhow::Result;
use asyncgit::{
	sync::{CommitId, RepoPathRef},
	AsyncGitNotification,
};
use crossbeam_channel::Sender;
use crossterm::event::Event;
use ratatui::{
	backend::Backend,
	layout::{Constraint, Layout, Rect},
	Frame,
};

use super::Revlog;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
	MainLog,
	CompareLog,
}

impl Focus {
	pub fn get_next(&self) -> Focus {
		match self {
			Focus::CompareLog => Focus::MainLog,
			Focus::MainLog => Focus::CompareLog,
		}
	}
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
	local_queue: SharedLocalQueue,
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
			local_queue: crate::queue::create_local_queue(),
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

	fn process_local_queue(&mut self) {
		let mut q = self.local_queue.borrow_mut();
		while let Some(e) = q.pop_front() {
			match e {
				LocalEvent::PickBranch(b) => self
					.compare_log
					.set_target_branch(Some((b.name, b.top_commit))),
			}
		}
	}

	///
	pub fn update(&mut self) -> Result<()> {
		if self.is_visible() {
			self.process_local_queue();
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

	fn set_focus(&mut self, f: Focus) {
		self.focused = f;
		self.compare_log.focus(self.focused == Focus::CompareLog);
		self.main_log.focus(self.focused == Focus::MainLog);
	}
}

impl DrawableComponent for RevlogExtern {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		if self.compare_log.is_visible() {
			//split in 2
			let v_blocks = Layout::default()
				.direction(ratatui::layout::Direction::Vertical)
				.constraints(
					[
						Constraint::Percentage(50),
						Constraint::Percentage(50),
					]
					.as_ref(),
				)
				.split(area);
			self.main_log.draw(f, v_blocks[0])?;
			self.compare_log.draw(f, v_blocks[1])?;
		} else {
			self.main_log.draw(f, area)?;
		}
		Ok(())
	}
}

impl Component for RevlogExtern {
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.visible {
			if let Event::Key(k) = ev {
				if key_match(k, self.key_config.keys.toggle_split) {
					if self.compare_log.is_visible() {
						self.compare_log.hide();
						self.set_focus(Focus::MainLog);
					} else {
						if let Ok(head) =
							asyncgit::sync::get_head_tuple_branch(
								&self.repo.borrow(),
							) {
							self.compare_log.set_target_branch(Some(
								(head.name, head.id),
							));
						}
						self.compare_log.show()?;
						self.set_focus(Focus::CompareLog);
					}
					return Ok(EventState::Consumed);
				} else if key_match(
					k,
					self.key_config.keys.toggle_workarea,
				) && self.compare_log.is_visible()
				{
					self.set_focus(self.focused.get_next());
					return Ok(EventState::Consumed);
				} else if self.focused == Focus::CompareLog
					&& key_match(
						k,
						self.key_config.keys.select_branch,
					) {
					//no checkout select branch
					self.queue.push(InternalEvent::PickBranch(
						self.local_queue.clone(),
					));
					return Ok(EventState::Consumed);
				}
			}

			return match self.focused {
				Focus::MainLog => self.main_log.event(ev),
				Focus::CompareLog => self.compare_log.event(ev),
			};
		}

		Ok(EventState::NotConsumed)
	}

	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		match self.focused {
			Focus::MainLog => self.main_log.commands(out, force_all),
			Focus::CompareLog => {
				self.compare_log.commands(out, force_all)
			}
		};
		visibility_blocking(self)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		self.main_log.show()?;
		self.update()?;

		Ok(())
	}
}
