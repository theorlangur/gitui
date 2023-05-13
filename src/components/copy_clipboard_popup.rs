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
use chrono::{DateTime, Local, NaiveDateTime, Utc};
use crossterm::event::Event;
use ratatui::{
	backend::Backend,
	layout::{Alignment, Rect},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};

#[derive(Debug)]
pub struct CopyClipboardOpen {
	pub commit_id: CommitId,
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
		self.copy_request = Some(CopyClipboardOpen {
			commit_id: copy.commit_id,
		});
		self.show()?;

		Ok(())
	}

	fn get_commit(&mut self) -> &CommitId {
		&self.copy_request.as_ref().unwrap().commit_id
	}

	fn get_commit_info(&mut self) -> Result<CommitInfo> {
		let oid = &self.copy_request.as_ref().unwrap().commit_id;
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
					crate::clipboard::copy_string(
						&self.get_commit().to_string(),
					)?;
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_email,
				) {
					crate::clipboard::copy_string(
						&self.get_commit_info()?.email,
					)?;
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_author,
				) {
					crate::clipboard::copy_string(
						&self.get_commit_info()?.author,
					)?;
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_message,
				) {
					crate::clipboard::copy_string(
						&self.get_commit_info()?.message,
					)?;
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_summary,
				) {
					let i = self.get_commit_info()?;
					let date =
						NaiveDateTime::from_timestamp_opt(i.time, 0);
					let dt = DateTime::<Local>::from(
						DateTime::<Utc>::from_utc(
							date.unwrap_or_default(),
							Utc,
						),
					);
					let summary = format!(
						"SHA: {}\nAuthor: {} <{}>\nDate: {}\n\n{}",
						self.get_commit().to_string(),
						i.author,
						i.email,
						dt,
						i.message
					);
					crate::clipboard::copy_string(&summary)?;
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
