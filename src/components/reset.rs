use crate::{
	components::{
		popup_paragraph, visibility_blocking, CommandBlocking,
		CommandInfo, Component, DrawableComponent, EventState,
	},
	keys::{key_match, SharedKeyConfig},
	queue::{
		Action, CustomConfirmData, InternalEvent, LocalEvent,
		NeedsUpdate, Queue,
	},
	strings, ui,
};
use anyhow::Result;
use crossterm::event::Event;
use ratatui::{
	backend::Backend, layout::Rect, text::Text, widgets::Clear, Frame,
};
use std::borrow::Cow;
use ui::style::SharedTheme;

///
pub struct ConfirmComponent {
	target: Option<Action>,
	visible: bool,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,

	custom: Option<CustomConfirmData>,
}

impl DrawableComponent for ConfirmComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		_rect: Rect,
	) -> Result<()> {
		if self.visible {
			let (title, msg) = self.get_text();

			let txt = Text::styled(
				Cow::from(msg),
				self.theme.text_danger(),
			);

			let area = ui::centered_rect(50, 20, f.size());
			f.render_widget(Clear, area);
			f.render_widget(
				popup_paragraph(&title, txt, &self.theme, true, true),
				area,
			);
		}

		Ok(())
	}
}

impl Component for ConfirmComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		out.push(CommandInfo::new(
			strings::commands::confirm_action(&self.key_config),
			true,
			self.visible,
		));
		out.push(CommandInfo::new(
			strings::commands::close_popup(&self.key_config),
			true,
			self.visible,
		));

		visibility_blocking(self)
	}

	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.visible {
			if let Event::Key(e) = ev {
				if key_match(e, self.key_config.keys.exit_popup) {
					self.hide();
				} else if key_match(e, self.key_config.keys.enter) {
					self.confirm();
				}

				return Ok(EventState::Consumed);
			}
		}

		Ok(EventState::NotConsumed)
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

impl ConfirmComponent {
	///
	pub fn new(
		queue: Queue,
		theme: SharedTheme,
		key_config: SharedKeyConfig,
	) -> Self {
		Self {
			target: None,
			visible: false,
			queue,
			theme,
			key_config,

			custom: None,
		}
	}
	///
	pub fn open(&mut self, a: Action) -> Result<()> {
		self.target = Some(a);
		self.custom = None;
		self.show()?;

		Ok(())
	}
	///
	pub fn open_custom(
		&mut self,
		c: CustomConfirmData,
	) -> Result<()> {
		self.target = None;
		self.custom = Some(c);
		self.show()?;

		Ok(())
	}
	///
	pub fn confirm(&mut self) {
		if let Some(c) = self.custom.as_mut() {
			let mut q = c.q.borrow_mut();
			q.push_back(LocalEvent::Confirmed(c.confirm.clone()));
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		} else if let Some(a) = self.target.take() {
			self.queue.push(InternalEvent::ConfirmedAction(a));
		}

		self.hide();
	}

	fn get_text(&self) -> (String, String) {
		if let Some(c) = self.custom.as_ref() {
			return (c.title.clone(), c.msg.clone());
		}
		if let Some(ref a) = self.target {
			return match a {
                Action::Reset(_) => (
                    strings::confirm_title_reset(),
                    strings::confirm_msg_reset(),
                ),
                Action::StashDrop(ids) => (
                    strings::confirm_title_stashdrop(
                        &self.key_config,ids.len()>1
                    ),
                    strings::confirm_msg_stashdrop(&self.key_config,ids),
                ),
                Action::StashPop(_) => (
                    strings::confirm_title_stashpop(&self.key_config),
                    strings::confirm_msg_stashpop(&self.key_config),
                ),
                Action::ResetHunk(_, _) => (
                    strings::confirm_title_reset(),
                    strings::confirm_msg_resethunk(&self.key_config),
                ),
                Action::ResetLines(_, lines) => (
                    strings::confirm_title_reset(),
                    strings::confirm_msg_reset_lines(lines.len()),
                ),
                Action::DeleteLocalBranch(branch_ref) => (
                    strings::confirm_title_delete_branch(
                        &self.key_config,
                    ),
                    strings::confirm_msg_delete_branch(
                        &self.key_config,
                        branch_ref,
                    ),
                ),
                Action::DeleteRemoteBranch(branch_ref) => (
                    strings::confirm_title_delete_remote_branch(
                        &self.key_config,
                    ),
                    strings::confirm_msg_delete_remote_branch(
                        &self.key_config,
                        branch_ref,
                    ),
                ),
                Action::DeleteTag(tag_name) => (
                    strings::confirm_title_delete_tag(
                        &self.key_config,
                    ),
                    strings::confirm_msg_delete_tag(
                        &self.key_config,
                        tag_name,
                    ),
                ),
				Action::DeleteRemoteTag(_tag_name,remote) => (
                    strings::confirm_title_delete_tag_remote(),
                    strings::confirm_msg_delete_tag_remote(remote),
                ),
                Action::ForcePush(branch, _force) => (
                    strings::confirm_title_force_push(
                        &self.key_config,
                    ),
                    strings::confirm_msg_force_push(
                        &self.key_config,
                        branch.rsplit('/').next().expect("There was no / in the head reference which is impossible in git"),
                    ),
                ),
                Action::PullMerge{incoming,rebase} => (
                    strings::confirm_title_merge(&self.key_config,*rebase),
                    strings::confirm_msg_merge(&self.key_config,*incoming,*rebase),
                ),
                Action::AbortMerge => (
                    strings::confirm_title_abortmerge(),
                    strings::confirm_msg_revertchanges(),
                ),
				Action::AbortRebase => (
                    strings::confirm_title_abortrebase(),
                    strings::confirm_msg_abortrebase(),
                ),
				Action::AbortRevert => (
                    strings::confirm_title_abortrevert(),
                    strings::confirm_msg_revertchanges(),
                ),
            };
		}

		(String::new(), String::new())
	}
}
