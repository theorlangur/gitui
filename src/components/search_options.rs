use super::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState,
};
use crate::{
	//components::utils::string_width_align,
	keys::{key_match, SharedKeyConfig},
	strings::{self},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use crossterm::event::Event;
use ratatui::{
	backend::Backend,
	layout::{Alignment, Rect},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear, Paragraph, /*, Paragraph*/},
	Frame,
};
//use scopeguard::defer;
//use std::io;

const FIELD_COUNT: usize = 2;

pub struct SearchOptionsPopupComponent {
	visible: bool,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
	pub author: bool,
	pub message: bool,
	selected_idx: usize,
}

impl SearchOptionsPopupComponent {
	///
	pub fn new(
		theme: SharedTheme,
		key_config: SharedKeyConfig,
	) -> Self {
		Self {
			visible: false,
			key_config,
			theme,
			author: true,
			message: true,
			selected_idx: 0,
		}
	}
	fn add_checkbox(
		&self,
		txt: &mut Vec<Spans>,
		name: String,
		state: bool,
		selected: bool,
	) {
		txt.push(Spans::from(vec![
			Span::styled(
				if state { "[X] " } else { "[ ] " },
				self.theme.text(true, false),
			),
			Span::styled(name, self.theme.text(true, selected)),
		]));
	}

	fn get_text(&self) -> Vec<Spans> {
		let mut txt: Vec<Spans> = Vec::with_capacity(10);
		self.add_checkbox(
			&mut txt,
			"Author".to_string(),
			self.author,
			self.selected_idx == 0,
		);
		self.add_checkbox(
			&mut txt,
			"Message".to_string(),
			self.message,
			self.selected_idx == 1,
		);

		txt
	}
}

impl DrawableComponent for SearchOptionsPopupComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		if self.is_visible() {
			let sz: (u16, u16) =
				((area.width as f32 * 0.75) as u16, 10);
			let area = ui::centered_rect_absolute(sz.0, sz.1, area);

			/*f.render_widget(Clear, area);
			f.render_widget(
				Block::default()
					.borders(Borders::ALL)
					.title(Span::styled(
						"Commit search options",
						self.theme.title(true),
					))
					.border_style(self.theme.block(true)),
				area,
			);*/

			f.render_widget(Clear, area);
			f.render_widget(
				Paragraph::new(self.get_text())
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

impl Component for SearchOptionsPopupComponent {
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
		}

		visibility_blocking(self)
	}

	fn event(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if self.is_visible() {
			let consumed = if let Event::Key(key) = &event {
				if key_match(key, self.key_config.keys.exit_popup) {
					self.hide();
					true
				} else if key_match(
					key,
					self.key_config.keys.stage_unstage_item,
				) {
					if self.selected_idx == 0 {
						self.author = !self.author;
					} else if self.selected_idx == 1 {
						self.message = !self.message;
					}
					true
				} else if key_match(
					key,
					self.key_config.keys.move_down,
				) {
					self.selected_idx =
						(self.selected_idx + 1) % FIELD_COUNT;
					true
				} else if key_match(key, self.key_config.keys.move_up)
				{
					self.selected_idx = (self.selected_idx
						+ FIELD_COUNT - 1)
						% FIELD_COUNT;
					true
				} else {
					false
				}
			} else {
				false
			};

			return Ok(consumed.into());
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
