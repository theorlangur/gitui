use super::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState,
};
use crate::{
	components::utils::string_width_align,
	keys::{key_match, SharedKeyConfig},
	strings::{self},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::sync::{
	get_commit_info, CommitId, CommitInfo, RepoPathRef,
};
use crossterm::event::Event;
use itertools::Itertools;
use ratatui::{
	backend::Backend,
	layout::{Alignment, Rect},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};

#[derive(Debug)]
pub struct CopyClipboardOpen {
	commit_ids: Vec<CommitId>,
}

impl CopyClipboardOpen {
	pub fn from_commit(c: CommitId) -> Self {
		Self {
			commit_ids: vec![c],
		}
	}

	pub fn new(commit_ids: Vec<CommitId>) -> Self {
		Self { commit_ids }
	}
}

pub struct CopyPopupComponent {
	visible: bool,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
	repo: RepoPathRef,
	copy_request: Option<CopyClipboardOpen>,
}

impl CopyPopupComponent {
	///
	pub fn new(
		theme: SharedTheme,
		key_config: SharedKeyConfig,
		repo: RepoPathRef,
	) -> Self {
		Self {
			visible: false,
			key_config,
			theme,
			repo,
			copy_request: None,
		}
	}

	pub fn open(&mut self, copy: CopyClipboardOpen) -> Result<()> {
		self.copy_request = Some(copy);
		self.show()?;

		Ok(())
	}

	fn get_commit_info(&self, oid: &CommitId) -> Result<CommitInfo> {
		Ok(get_commit_info(&self.repo.borrow(), oid)?)
	}

	fn get_text(&self, width: u16) -> Vec<Spans> {
		let mut txt: Vec<Spans> = Vec::with_capacity(10);

		self.add_status(&mut txt, width);

		txt
	}

	fn add_status(&self, txt: &mut Vec<Spans>, width: u16) {
		txt.push(Spans::from(vec![Span::raw("")]));

		self.add_action(txt, width, "s", "Copy SHA");
		self.add_action(txt, width, "e", "Copy e-mail");
		self.add_action(txt, width, "a", "Copy author");
		self.add_action(txt, width, "m", "Copy message");
		self.add_action(txt, width, "S", "Copy summary");
	}

	fn add_action(
		&self,
		txt: &mut Vec<Spans>,
		width: u16,
		action_key: &str,
		value: &str,
	) {
		let key_size = 3;
		let rest_width = usize::from((width - key_size as u16) / 2);
		txt.push(Spans::from(vec![
			Span::styled(
				string_width_align(action_key, key_size),
				self.theme.text(true, false),
			),
			Span::styled(
				format!("{value:<rest_width$}"),
				self.theme.text(true, false),
			),
		]));
	}
}

impl DrawableComponent for CopyPopupComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		if self.is_visible() {
			const SIZE: (u16, u16) = (50, 10);
			let area =
				ui::centered_rect_absolute(SIZE.0, SIZE.1, area);

			let width = area.width;

			f.render_widget(Clear, area);
			f.render_widget(
				Paragraph::new(self.get_text(width))
					.block(
						Block::default()
							.borders(Borders::ALL)
							.title(Span::styled(
								"Copy",
								self.theme.title(true),
							))
							.border_style(self.theme.block(true)),
					)
					.alignment(Alignment::Left),
				area,
			);
		}

		Ok(())
	}
}

impl Component for CopyPopupComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			out.push(CommandInfo::new(
				strings::commands::close_popup(&self.key_config),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::copy_clipboard_sha(
					&self.key_config,
				),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::copy_clipboard_email(
					&self.key_config,
				),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::copy_clipboard_author(
					&self.key_config,
				),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::copy_clipboard_message(
					&self.key_config,
				),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::copy_clipboard_summary(
					&self.key_config,
				),
				true,
				true,
			));
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.is_visible() {
			if let Event::Key(key) = &event {
				if key_match(key, self.key_config.keys.exit_popup) {
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_sha,
				) {
					if let Some(r) = &self.copy_request {
						crate::clipboard::copy_string(
							&r.commit_ids
								.iter()
								.map(|i| i.to_string())
								.join("\n"),
						)?;
					}
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_email,
				) {
					if let Some(r) = &self.copy_request {
						crate::clipboard::copy_string(
							&r.commit_ids
								.iter()
								.filter_map(|i| {
									self.get_commit_info(i).ok()
								})
								.map(|i| i.email)
								.join("\n"),
						)?;
					}
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_author,
				) {
					if let Some(r) = &self.copy_request {
						crate::clipboard::copy_string(
							&r.commit_ids
								.iter()
								.filter_map(|i| {
									self.get_commit_info(i).ok()
								})
								.map(|i| i.author)
								.join("\n"),
						)?;
					}
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_message,
				) {
					if let Some(r) = &self.copy_request {
						crate::clipboard::copy_string(
							&r.commit_ids
								.iter()
								.filter_map(|i| {
									self.get_commit_info(i).ok()
								})
								.map(|i| i.message)
								.join("\n"),
						)?;
					}
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_summary,
				) {
					if let Some(r) = &self.copy_request {
						crate::clipboard::copy_string(
							&r.commit_ids
								.iter()
								.filter_map(|i| {
									self.get_commit_info(i).ok()
								})
								.map(|i| i.get_clipboard_summary())
								.join("\n"),
						)?;
					}
					self.hide();
				}
			}

			return Ok(EventState::Consumed);
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
