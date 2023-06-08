use super::{
	utils::scroll_vertical::VerticalScroll, visibility_blocking,
	CommandBlocking, CommandInfo, Component, DrawableComponent,
	EventState, InspectCommitOpen,
};
use crate::{
	components::ScrollType,
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::{
		Action, InternalEvent, NeedsUpdate, Queue, StackablePopupOpen,
	},
	strings, try_or_popup,
	ui::{self, show_message_in_center, Size},
};
use anyhow::Result;
use asyncgit::{
	sync::{
		self,
		branch::{
			checkout_branch_cmd, checkout_remote_branch,
			BranchDetails, LocalBranch, RemoteBranch,
		},
		checkout_branch, get_branches_info, BranchInfo, BranchType,
		CommitId, RepoPathRef, RepoState,
	},
	AsyncGitNotification,
};
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
	backend::Backend,
	layout::{
		Alignment, Constraint, Direction, Layout, Margin, Rect,
	},
	text::{Span, Spans, Text},
	widgets::{Block, BorderType, Borders, Clear, Paragraph, Tabs},
	Frame,
};
use std::{cell::Cell, convert::TryInto};
use ui::style::SharedTheme;
use unicode_truncate::UnicodeTruncateStr;

#[derive(PartialEq)]
enum ShortcutState {
	Idle,
	AssignNew,
	Trigger,
}

///
pub struct BranchListComponent {
	repo: RepoPathRef,
	branches: Vec<BranchInfo>,
	local: bool,
	has_remotes: bool,
	visible: bool,
	selection: u16,
	scroll: VerticalScroll,
	current_height: Cell<u16>,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	options: SharedOptions,
	shortcut_state: ShortcutState,
}

impl DrawableComponent for BranchListComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		rect: Rect,
	) -> Result<()> {
		if self.is_visible() {
			const PERCENT_SIZE: Size = Size::new(80, 50);
			const MIN_SIZE: Size = Size::new(60, 20);

			let msg_rect = ui::centered_rect_absolute(
				PERCENT_SIZE.width / 2,
				3,
				rect,
			);

			let area = ui::centered_rect(
				PERCENT_SIZE.width,
				PERCENT_SIZE.height,
				f.size(),
			);
			let area =
				ui::rect_inside(MIN_SIZE, f.size().into(), area);
			let area = area.intersection(rect);

			f.render_widget(Clear, area);

			f.render_widget(
				Block::default()
					.title(strings::title_branches())
					.border_type(BorderType::Thick)
					.borders(Borders::ALL),
				area,
			);

			let area = area.inner(&Margin {
				vertical: 1,
				horizontal: 1,
			});

			let chunks = Layout::default()
				.direction(Direction::Vertical)
				.constraints(
					[Constraint::Length(2), Constraint::Min(1)]
						.as_ref(),
				)
				.split(area);

			self.draw_tabs(f, chunks[0]);
			self.draw_list(f, chunks[1])?;

			if self.shortcut_state == ShortcutState::AssignNew {
				show_message_in_center(
					f,
					&self.theme,
					msg_rect,
					"Hit a key to assign a shortcut...",
				);
			}
		}

		Ok(())
	}
}

impl Component for BranchListComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.visible || force_all {
			if !force_all {
				out.clear();
			}

			out.push(CommandInfo::new(
				strings::commands::scroll(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::close_popup(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::commit_details_open(
					&self.key_config,
				),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::compare_with_head(
					&self.key_config,
				),
				!self.selection_is_cur_branch(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::toggle_branch_popup(
					&self.key_config,
					self.local,
				),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::select_branch_popup(
					&self.key_config,
				),
				!self.selection_is_cur_branch()
					&& self.valid_selection(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::open_branch_create_popup(
					&self.key_config,
				),
				true,
				self.local,
			));

			out.push(CommandInfo::new(
				strings::commands::delete_branch_popup(
					&self.key_config,
				),
				!self.selection_is_cur_branch(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::merge_branch_popup(
					&self.key_config,
				),
				!self.selection_is_cur_branch(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::branch_popup_rebase(
					&self.key_config,
				),
				!self.selection_is_cur_branch(),
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::rename_branch_popup(
					&self.key_config,
				),
				true,
				self.local,
			));

			out.push(CommandInfo::new(
				strings::commands::fetch_remotes(&self.key_config),
				self.has_remotes,
				!self.local,
			));

			out.push(CommandInfo::new(
				strings::commands::find_branch(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::assign_shortcut(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::clear_shortcut(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::clear_all_shortcuts(
					&self.key_config,
				),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::checkout_branch_by_shortcut(
					&self.key_config,
				),
				true,
				true,
			));
		}
		visibility_blocking(self)
	}

	//TODO: cleanup
	#[allow(clippy::cognitive_complexity)]
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if !self.visible {
			return Ok(EventState::NotConsumed);
		}

		if let Event::Key(e) = ev {
			match self.shortcut_state {
				ShortcutState::AssignNew => {
					self.shortcut_state = ShortcutState::Idle;
					if self.valid_selection()
						&& e.code != KeyCode::Esc
					{
						self.options
							.borrow_mut()
							.assign_shortcut_for_branch(
								&self.branches
									[self.selection as usize]
									.name,
								e,
							);
					}
					return Ok(EventState::Consumed);
				}
				ShortcutState::Trigger => {
					self.shortcut_state = ShortcutState::Idle;
					let opts = self.options.borrow();
					let shortcut = opts.find_branch_by_key_event(e);
					if let Some(branch) = shortcut {
						let b_index = self
							.branches
							.iter()
							.enumerate()
							.find(|i| i.1.name == branch)
							.map(|i| i.0);
						drop(opts);
						if let Some(b_index) = b_index {
							self.selection = b_index as u16;
							self.switch_to_selected_branch()?;
						}
					}
					return Ok(EventState::Consumed);
				}
				_ => {}
			}

			if self.move_event(e)?.is_consumed() {
				return Ok(EventState::Consumed);
			}

			if key_match(e, self.key_config.keys.enter) {
				try_or_popup!(
					self,
					"switch branch error:",
					self.switch_to_selected_branch()
				);
			} else if key_match(e, self.key_config.keys.create_branch)
				&& self.local
			{
				self.queue.push(InternalEvent::CreateBranch);
			} else if key_match(e, self.key_config.keys.rename_branch)
				&& self.valid_selection()
			{
				self.rename_branch();
			} else if key_match(e, self.key_config.keys.delete_branch)
				&& !self.selection_is_cur_branch()
				&& self.valid_selection()
			{
				self.delete_branch();
			} else if key_match(e, self.key_config.keys.merge_branch)
				&& !self.selection_is_cur_branch()
				&& self.valid_selection()
			{
				try_or_popup!(
					self,
					"merge branch error:",
					self.merge_branch()
				);
			} else if key_match(e, self.key_config.keys.rebase_branch)
				&& !self.selection_is_cur_branch()
				&& self.valid_selection()
			{
				try_or_popup!(
					self,
					"rebase error:",
					self.rebase_branch()
				);
			} else if key_match(e, self.key_config.keys.move_right)
				&& self.valid_selection()
			{
				self.inspect_head_of_branch();
			} else if key_match(
				e,
				self.key_config.keys.compare_commits,
			) && self.valid_selection()
			{
				self.hide();
				if let Some(commit_id) = self.get_selected() {
					self.queue.push(InternalEvent::OpenPopup(
						StackablePopupOpen::CompareCommits(
							InspectCommitOpen::new(commit_id),
						),
					));
				}
			} else if key_match(e, self.key_config.keys.fetch)
				&& !self.local && self.has_remotes
			{
				self.queue.push(InternalEvent::FetchRemotes);
			} else if key_match(
				e,
				self.key_config.keys.cmd_bar_toggle,
			) {
				//do not consume if its the more key
				return Ok(EventState::NotConsumed);
			} else if key_match(
				e,
				self.key_config.keys.assign_shortcut,
			) && self.valid_selection()
			{
				//start shortcut assignment
				self.shortcut_state = ShortcutState::AssignNew;
			} else if key_match(
				e,
				self.key_config.keys.clear_shortcut,
			) && self.valid_selection()
			{
				self.options.borrow_mut().remove_shortcut_for_branch(
					&self.branches[self.selection as usize].name,
				);
			} else if key_match(
				e,
				self.key_config.keys.clear_all_shortcut,
			) && self.valid_selection()
			{
				self.options
					.borrow_mut()
					.clear_all_branch_shortcuts();
			} else if key_match(
				e,
				self.key_config.keys.trigger_branch_shortcut,
			) {
				self.shortcut_state = ShortcutState::Trigger;
			} else if key_match(
				e,
				self.key_config.keys.start_search_forward_init,
			) {
				let branches = self
					.branches
					.iter()
					.map(|b| b.name.clone())
					.collect();
				self.queue
					.push(InternalEvent::OpenBranchFinder(branches));
			}
		}

		Ok(EventState::Consumed)
	}

	fn is_visible(&self) -> bool {
		self.visible
	}

	fn hide(&mut self) {
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;

		Ok(())
	}
}

impl BranchListComponent {
	pub fn new(
		repo: RepoPathRef,
		queue: Queue,
		theme: SharedTheme,
		key_config: SharedKeyConfig,
		options: SharedOptions,
	) -> Self {
		Self {
			branches: Vec::new(),
			local: true,
			has_remotes: false,
			visible: false,
			selection: 0,
			scroll: VerticalScroll::new(),
			queue,
			theme,
			key_config,
			current_height: Cell::new(0),
			repo,
			shortcut_state: ShortcutState::Idle,
			options,
		}
	}

	fn move_event(&mut self, e: &KeyEvent) -> Result<EventState> {
		if key_match(e, self.key_config.keys.exit_popup) {
			self.hide();
		} else if key_match(e, self.key_config.keys.move_down) {
			return self
				.move_selection(ScrollType::Up)
				.map(Into::into);
		} else if key_match(e, self.key_config.keys.move_up) {
			return self
				.move_selection(ScrollType::Down)
				.map(Into::into);
		} else if key_match(e, self.key_config.keys.page_down) {
			return self
				.move_selection(ScrollType::PageDown)
				.map(Into::into);
		} else if key_match(e, self.key_config.keys.page_up) {
			return self
				.move_selection(ScrollType::PageUp)
				.map(Into::into);
		} else if key_match(e, self.key_config.keys.home) {
			return self
				.move_selection(ScrollType::Home)
				.map(Into::into);
		} else if key_match(e, self.key_config.keys.end) {
			return self
				.move_selection(ScrollType::End)
				.map(Into::into);
		} else if key_match(e, self.key_config.keys.tab_toggle) {
			self.local = !self.local;
			self.check_remotes();
			self.update_branches()?;
		}
		Ok(EventState::NotConsumed)
	}

	///
	pub fn open(&mut self) -> Result<()> {
		self.show()?;
		self.update_branches()?;

		Ok(())
	}

	pub fn branch_finder_update(
		&mut self,
		idx: Option<usize>,
	) -> Result<()> {
		if let Some(idx) = idx {
			self.set_selection(idx.try_into()?)?;
		}
		Ok(())
	}

	fn check_remotes(&mut self) {
		if !self.local && self.visible {
			self.has_remotes =
				get_branches_info(&self.repo.borrow(), false)
					.map(|branches| !branches.is_empty())
					.unwrap_or(false);
		}
	}

	fn update_auto_shortcuts(&mut self) {
		//self.auto_shortcuts.clear();
		//self.auto_shortcuts.reserve(self.branches.len());
	}

	/// fetch list of branches
	pub fn update_branches(&mut self) -> Result<()> {
		if self.is_visible() {
			self.check_remotes();
			self.branches =
				get_branches_info(&self.repo.borrow(), self.local)?;
			//remove remote branch called `HEAD`
			if !self.local {
				self.branches
					.iter()
					.position(|b| b.name.ends_with("/HEAD"))
					.map(|idx| self.branches.remove(idx));
			}
			self.set_selection(self.selection)?;
			self.update_auto_shortcuts();
		}
		Ok(())
	}

	///
	pub fn update_git(
		&mut self,
		ev: AsyncGitNotification,
	) -> Result<()> {
		if self.is_visible() && ev == AsyncGitNotification::Push {
			self.update_branches()?;
		}

		Ok(())
	}

	fn valid_selection(&self) -> bool {
		!self.branches.is_empty()
	}

	fn merge_branch(&mut self) -> Result<()> {
		if let Some(branch) =
			self.branches.get(usize::from(self.selection))
		{
			sync::merge_branch(
				&self.repo.borrow(),
				&branch.name,
				self.get_branch_type(),
			)?;

			self.hide_and_switch_tab()?;
		}

		Ok(())
	}

	fn rebase_branch(&mut self) -> Result<()> {
		if let Some(branch) =
			self.branches.get(usize::from(self.selection))
		{
			sync::rebase_branch(
				&self.repo.borrow(),
				&branch.name,
				self.get_branch_type(),
			)?;

			self.hide_and_switch_tab()?;
		}

		Ok(())
	}

	fn inspect_head_of_branch(&mut self) {
		if let Some(commit_id) = self.get_selected() {
			self.hide();
			self.queue.push(InternalEvent::OpenPopup(
				StackablePopupOpen::InspectCommit(
					InspectCommitOpen::new(commit_id),
				),
			));
		}
	}

	const fn get_branch_type(&self) -> BranchType {
		if self.local {
			BranchType::Local
		} else {
			BranchType::Remote
		}
	}

	fn hide_and_switch_tab(&mut self) -> Result<()> {
		self.hide();
		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));

		if sync::repo_state(&self.repo.borrow())? != RepoState::Clean
		{
			self.queue.push(InternalEvent::TabSwitchStatus);
		}

		Ok(())
	}

	fn selection_is_cur_branch(&self) -> bool {
		self.branches
			.iter()
			.enumerate()
			.filter(|(index, b)| {
				b.local_details()
					.map(|details| {
						details.is_head
							&& *index == self.selection as usize
					})
					.unwrap_or_default()
			})
			.count() > 0
	}

	fn get_selected(&self) -> Option<CommitId> {
		self.branches
			.get(usize::from(self.selection))
			.map(|b| b.top_commit)
	}

	///
	fn move_selection(&mut self, scroll: ScrollType) -> Result<bool> {
		let new_selection = match scroll {
			ScrollType::Up => self.selection.saturating_add(1),
			ScrollType::Down => self.selection.saturating_sub(1),
			ScrollType::PageDown => self
				.selection
				.saturating_add(self.current_height.get()),
			ScrollType::PageUp => self
				.selection
				.saturating_sub(self.current_height.get()),
			ScrollType::Home => 0,
			ScrollType::End => {
				let num_branches: u16 =
					self.branches.len().try_into()?;
				num_branches.saturating_sub(1)
			}
		};

		self.set_selection(new_selection)?;

		Ok(true)
	}

	fn set_selection(&mut self, selection: u16) -> Result<()> {
		let num_branches: u16 = self.branches.len().try_into()?;
		let num_branches = num_branches.saturating_sub(1);

		let selection = if selection > num_branches {
			num_branches
		} else {
			selection
		};

		self.selection = selection;

		Ok(())
	}

	/// Get branches to display
	fn get_text(
		&self,
		theme: &SharedTheme,
		width_available: u16,
		height: usize,
	) -> Text {
		const UPSTREAM_SYMBOL: char = '\u{2191}';
		const TRACKING_SYMBOL: char = '\u{2193}';
		const HEAD_SYMBOL: char = '*';
		const EMPTY_SYMBOL: char = ' ';
		const THREE_DOTS: &str = "...";
		const THREE_DOTS_LENGTH: usize = THREE_DOTS.len(); // "..."
		const COMMIT_HASH_LENGTH: usize = 8;
		const IS_HEAD_STAR_LENGTH: usize = 3; // "*  "
		const SHORTCUT_WIDTH: usize = 4; // "*  "

		let opts = self.options.borrow();
		let has_shortcuts = opts.has_any_branch_shortcuts();

		let branch_name_length: usize =
			(width_available as usize * 40 / 100)
				- (has_shortcuts as usize * SHORTCUT_WIDTH);
		// commit message takes up the remaining width
		let commit_message_length: usize = (width_available as usize)
			.saturating_sub(COMMIT_HASH_LENGTH)
			.saturating_sub(branch_name_length)
			.saturating_sub(IS_HEAD_STAR_LENGTH)
			.saturating_sub(THREE_DOTS_LENGTH);
		let mut txt = Vec::new();

		for (i, displaybranch) in self
			.branches
			.iter()
			.skip(self.scroll.get_top())
			.take(height)
			.enumerate()
		{
			let mut commit_message =
				displaybranch.top_commit_message.clone();
			if commit_message.len() > commit_message_length {
				commit_message.unicode_truncate(
					commit_message_length
						.saturating_sub(THREE_DOTS_LENGTH),
				);
				commit_message += THREE_DOTS;
			}

			let shortcut = opts
				.find_branch_shortcut_by_branch(&displaybranch.name);
			let mut shortcut = if let Some(s) = shortcut {
				self.key_config.get_hint(*s)
			} else {
				String::from(" ")
			};

			let mut branch_name = displaybranch.name.clone();
			if branch_name.len()
				> branch_name_length.saturating_sub(THREE_DOTS_LENGTH)
			{
				branch_name = branch_name
					.unicode_truncate(
						branch_name_length
							.saturating_sub(THREE_DOTS_LENGTH),
					)
					.0
					.to_string();
				branch_name += THREE_DOTS;
			}

			let selected = (self.selection as usize
				- self.scroll.get_top())
				== i;

			let is_head = displaybranch
				.local_details()
				.map(|details| details.is_head)
				.unwrap_or_default();
			let is_head_str =
				if is_head { HEAD_SYMBOL } else { EMPTY_SYMBOL };
			let upstream_tracking_str = match displaybranch.details {
				BranchDetails::Local(LocalBranch {
					has_upstream,
					..
				}) if has_upstream => UPSTREAM_SYMBOL,
				BranchDetails::Remote(RemoteBranch {
					has_tracking,
					..
				}) if has_tracking => TRACKING_SYMBOL,
				_ => EMPTY_SYMBOL,
			};

			let span_prefix = Span::styled(
				format!("{is_head_str}{upstream_tracking_str} "),
				theme.commit_author(selected),
			);
			let span_hash = Span::styled(
				format!(
					"{} ",
					displaybranch.top_commit.get_short_string()
				),
				theme.commit_hash(selected),
			);
			let span_msg = Span::styled(
				commit_message.to_string(),
				theme.text(true, selected),
			);
			let span_name = Span::styled(
				format!("{branch_name:branch_name_length$} "),
				theme.branch(selected, is_head),
			);

			if has_shortcuts {
				shortcut.extend(
					[' '].iter().cycle().take(
						SHORTCUT_WIDTH - shortcut.chars().count(),
					),
				);
				let span_shortcut = Span::styled(
					shortcut,
					theme.branch(selected, is_head),
				);
				txt.push(Spans::from(vec![
					span_prefix,
					span_shortcut,
					span_name,
					span_hash,
					span_msg,
				]));
			} else {
				txt.push(Spans::from(vec![
					span_prefix,
					span_name,
					span_hash,
					span_msg,
				]));
			}
		}

		Text::from(txt)
	}

	///
	fn switch_to_selected_branch(&mut self) -> Result<()> {
		if !self.valid_selection() {
			anyhow::bail!("no valid branch selected");
		}
		let cmd = self
			.options
			.borrow()
			.git_extern_commands()
			.checkout_base
			.as_ref()
			.map_or(String::new(), |i| i.clone());

		if !cmd.is_empty() {
			let branch = if self.local {
				self.branches[self.selection as usize].name.clone()
			} else {
				let branch = &self.branches[self.selection as usize];
				branch.name.find('/').map_or_else(
					|| branch.name.clone(),
					|pos| branch.name[pos..].to_string(),
				)
			};
			let r = checkout_branch_cmd(cmd, &branch);
			if let Err(e) = r {
				self.queue.push(
					crate::queue::InternalEvent::ShowErrorMsg(
						format!("{}\n{}", "Checkout failed", e),
					),
				);
				self.hide();
			} else if self.local {
				self.hide();
			} else {
				self.local = true;
				self.update_branches()?;
			}
		} else {
			if self.local {
				checkout_branch(
					&self.repo.borrow(),
					&self.branches[self.selection as usize].reference,
				)?;
				self.hide();
			} else {
				checkout_remote_branch(
					&self.repo.borrow(),
					&self.branches[self.selection as usize],
				)?;
				self.local = true;
				self.update_branches()?;
			}
		}

		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));

		Ok(())
	}

	fn draw_tabs<B: Backend>(&self, f: &mut Frame<B>, r: Rect) {
		let tabs = [Span::raw("Local"), Span::raw("Remote")]
			.iter()
			.cloned()
			.map(Spans::from)
			.collect();

		f.render_widget(
			Tabs::new(tabs)
				.block(
					Block::default()
						.borders(Borders::BOTTOM)
						.border_style(self.theme.block(false)),
				)
				.style(self.theme.tab(false))
				.highlight_style(self.theme.tab(true))
				.divider(strings::tab_divider(&self.key_config))
				.select(if self.local { 0 } else { 1 }),
			r,
		);
	}

	fn draw_list<B: Backend>(
		&self,
		f: &mut Frame<B>,
		r: Rect,
	) -> Result<()> {
		let height_in_lines = r.height as usize;
		self.current_height.set(height_in_lines.try_into()?);

		self.scroll.update(
			self.selection as usize,
			self.branches.len(),
			height_in_lines,
		);

		f.render_widget(
			Paragraph::new(self.get_text(
				&self.theme,
				r.width,
				height_in_lines,
			))
			.alignment(Alignment::Left),
			r,
		);

		let mut r = r;
		r.width += 1;
		r.height += 2;
		r.y = r.y.saturating_sub(1);

		self.scroll.draw(f, r, &self.theme);

		Ok(())
	}

	fn rename_branch(&mut self) {
		let cur_branch = &self.branches[self.selection as usize];
		self.queue.push(InternalEvent::RenameBranch(
			cur_branch.reference.clone(),
			cur_branch.name.clone(),
		));
	}

	fn delete_branch(&mut self) {
		let reference =
			self.branches[self.selection as usize].reference.clone();

		self.queue.push(InternalEvent::ConfirmAction(
			if self.local {
				Action::DeleteLocalBranch(reference)
			} else {
				Action::DeleteRemoteBranch(reference)
			},
		));
	}
}
