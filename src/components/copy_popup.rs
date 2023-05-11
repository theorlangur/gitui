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
use crossterm::event::Event;
use ratatui::{
	backend::Backend,
	layout::{Alignment, Rect},
	style::{Modifier, Style},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear, Paragraph},
	Frame,
};

pub struct CopyPopupComponent {
	visible: bool,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
}

impl CopyPopupComponent {
	///
	pub fn new(
		theme: SharedTheme,
		key_config: SharedKeyConfig,
	) -> Self {
		Self {
			visible: false,
			key_config,
			theme,
		}
	}

	fn get_text(&self, width: u16) -> Vec<Spans> {
		let mut txt: Vec<Spans> = Vec::with_capacity(10);

		self.add_status(&mut txt, width);

		txt
	}

	fn add_header(txt: &mut Vec<Spans>, header: &'static str) {
		txt.push(Spans::from(vec![Span::styled(
			header,
			//TODO: use style
			Style::default().add_modifier(Modifier::UNDERLINED),
		)]));
	}

	fn add_status(&self, txt: &mut Vec<Spans>, width: u16) {
		Self::add_header(txt, "Copy");

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
								"Options",
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
			out.push(
				CommandInfo::new(
					strings::commands::close_popup(&self.key_config),
					true,
					true,
				)
				.order(1),
			);
			out.push(
				CommandInfo::new(
					strings::commands::navigate_tree(
						&self.key_config,
					),
					true,
					true,
				)
				.order(1),
			);
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
					todo!("copy clipboard sha")
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_email,
				) {
					todo!("copy clipboard email")
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_author,
				) {
					todo!("copy clipboard author")
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_message,
				) {
					todo!("copy clipboard message")
				} else if key_match(
					key,
					self.key_config.keys.copy_clipboard_summary,
				) {
					todo!("copy clipboard summary")
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
