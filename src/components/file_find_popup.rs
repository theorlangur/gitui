use super::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, ScrollType, TextInputComponent,
};
use crate::{
	keys::{key_match, SharedKeyConfig},
	queue::{
		InternalEvent, LocalEvent, NeedsUpdate, Queue,
		SharedLocalQueue,
	},
	string_utils::trim_length_left,
	strings,
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::sync::TreeFile;
use crossterm::event::Event;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{
	backend::Backend,
	layout::{Constraint, Direction, Layout, Margin, Rect},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear},
	Frame,
};
use std::borrow::Cow;

#[derive(Eq, PartialEq)]
enum Focus {
	Input,
	List,
}

pub struct FileFindPopup {
	queue: Queue,
	visible: bool,
	find_text: TextInputComponent,
	query: Option<String>,
	theme: SharedTheme,
	files: Vec<TreeFile>,
	selection: usize,
	selected_index: Option<usize>,
	files_filtered: Vec<(usize, Vec<usize>)>,
	key_config: SharedKeyConfig,
	response_queue: Option<SharedLocalQueue>,
	focused: Focus,
}

impl FileFindPopup {
	///
	pub fn new(
		queue: &Queue,
		theme: SharedTheme,
		key_config: SharedKeyConfig,
	) -> Self {
		let mut find_text = TextInputComponent::new(
			theme.clone(),
			key_config.clone(),
			"",
			"start typing..",
			false,
		);
		find_text.embed();
		find_text.set_selected(true);

		Self {
			queue: queue.clone(),
			visible: false,
			query: None,
			find_text,
			theme,
			files: Vec::new(),
			files_filtered: Vec::new(),
			selected_index: None,
			key_config,
			selection: 0,
			response_queue: None,
			focused: Focus::Input,
		}
	}

	fn update_query(&mut self) {
		if self.find_text.get_text().is_empty() {
			self.set_query(None);
		} else if self
			.query
			.as_ref()
			.map_or(true, |q| q != self.find_text.get_text())
		{
			self.set_query(Some(
				self.find_text.get_text().to_string(),
			));
		}
	}

	fn set_query(&mut self, query: Option<String>) {
		self.query = query;

		self.files_filtered.clear();

		if let Some(q) = &self.query {
			let matcher =
				fuzzy_matcher::skim::SkimMatcherV2::default();

			let mut files = self
				.files
				.iter()
				.enumerate()
				.filter_map(|a| {
					a.1.path.to_str().and_then(|path| {
						matcher.fuzzy_indices(path, q).map(
							|(score, indices)| (score, a.0, indices),
						)
					})
				})
				.collect::<Vec<(_, _, _)>>();

			files.sort_by(|(score1, _, _), (score2, _, _)| {
				score2.cmp(score1)
			});

			self.files_filtered.extend(
				files.into_iter().map(|entry| (entry.1, entry.2)),
			);
		}

		self.selection = 0;
		self.refresh_selection();
	}

	fn refresh_selection(&mut self) {
		let selection =
			self.files_filtered.get(self.selection).map(|a| a.0);

		if self.selected_index != selection {
			self.selected_index = selection;

			let file = self
				.selected_index
				.and_then(|index| self.files.get(index))
				.map(|f| f.path.clone());

			if self.response_queue.is_none() {
				self.queue
					.push(InternalEvent::FileFinderChanged(file));
			}
		}
	}

	fn finish_selection(&mut self) {
		if let Some(q) = self.response_queue.as_mut() {
			let file = self
				.selected_index
				.and_then(|index| self.files.get(index))
				.map(|f| f.path.clone());
			q.borrow_mut().push_back(LocalEvent::PickFile(
				file.unwrap_or_default(),
			));
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		}
	}

	pub fn open(
		&mut self,
		files: &[TreeFile],
		queue: Option<SharedLocalQueue>,
	) -> Result<()> {
		self.show()?;
		self.response_queue = queue;
		self.find_text.show()?;
		self.find_text.set_text(String::new());
		self.query = None;
		if self.files != *files {
			self.files = files.to_owned();
		}
		self.update_query();

		Ok(())
	}

	fn move_selection(&mut self, move_type: ScrollType) -> bool {
		let new_selection = match move_type {
			ScrollType::Up => self.selection.saturating_sub(1),
			ScrollType::Down => self.selection.saturating_add(1),
			ScrollType::Home => 0,
			ScrollType::End => {
				self.files_filtered.len().saturating_sub(1)
			}
			_ => self.selection,
		};

		let new_selection = new_selection
			.clamp(0, self.files_filtered.len().saturating_sub(1));

		if new_selection != self.selection {
			self.selection = new_selection;
			self.refresh_selection();
			return true;
		}

		false
	}

	fn list_event(
		&mut self,
		key: &crossterm::event::KeyEvent,
	) -> Result<EventState> {
		if key_match(key, self.key_config.keys.move_down) {
			self.move_selection(ScrollType::Down);
		} else if key_match(key, self.key_config.keys.move_up) {
			self.move_selection(ScrollType::Up);
		} else if key_match(key, self.key_config.keys.home) {
			self.move_selection(ScrollType::Home);
		} else if key_match(key, self.key_config.keys.end) {
			self.move_selection(ScrollType::End);
		}
		Ok(EventState::Consumed)
	}
}

impl DrawableComponent for FileFindPopup {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		if self.is_visible() {
			const MAX_SIZE: (u16, u16) = (50, 20);

			let any_hits = !self.files_filtered.is_empty();

			let area = ui::centered_rect_absolute(
				MAX_SIZE.0, MAX_SIZE.1, area,
			);

			let area = if any_hits {
				area
			} else {
				Layout::default()
					.direction(Direction::Vertical)
					.constraints(
						[
							Constraint::Length(3),
							Constraint::Percentage(100),
						]
						.as_ref(),
					)
					.split(area)[0]
			};

			f.render_widget(Clear, area);
			f.render_widget(
				Block::default()
					.borders(Borders::all())
					.style(self.theme.title(true))
					.title(Span::styled(
						strings::POPUP_TITLE_FUZZY_FIND,
						self.theme.title(true),
					)),
				area,
			);

			let chunks = Layout::default()
				.direction(Direction::Vertical)
				.constraints(
					[
						Constraint::Length(1),
						Constraint::Percentage(100),
					]
					.as_ref(),
				)
				.split(area.inner(&Margin {
					horizontal: 1,
					vertical: 1,
				}));

			self.find_text.draw(f, chunks[0])?;

			if any_hits {
				let title =
					format!("Hits: {}", self.files_filtered.len());

				let height = usize::from(chunks[1].height);
				let width = usize::from(chunks[1].width);
				let items_height = height - 1;
				let skip = if items_height > 0 {
					(self.selection / items_height) * items_height
				} else {
					0
				};

				let items = self
					.files_filtered
					.iter()
					.skip(skip)
					.take(items_height)
					.map(|(idx, indicies)| {
						let selected = self
							.selected_index
							.map_or(false, |index| index == *idx);
						let full_text = trim_length_left(
							self.files[*idx]
								.path
								.to_str()
								.unwrap_or_default(),
							width,
						);
						Spans::from(
							full_text
								.char_indices()
								.map(|(c_idx, c)| {
									Span::styled(
										Cow::from(c.to_string()),
										self.theme.text(
											selected,
											indicies.contains(&c_idx),
										),
									)
								})
								.collect::<Vec<_>>(),
						)
					});

				ui::draw_list_block(
					f,
					chunks[1],
					Block::default()
						.title(Span::styled(
							title,
							self.theme
								.title(self.focused == Focus::List),
						))
						.borders(Borders::TOP),
					items,
				);
			}
		}
		Ok(())
	}
}

impl Component for FileFindPopup {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		force_all: bool,
	) -> CommandBlocking {
		if self.is_visible() || force_all {
			out.push(CommandInfo::new(
				strings::commands::diff_home_end(&self.key_config),
				true,
				self.focused == Focus::List,
			));

			out.push(CommandInfo::new(
				strings::commands::switch_focus(&self.key_config),
				true,
				true,
			));

			out.push(CommandInfo::new(
				strings::commands::close_fuzzy_finder(
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
			if let Event::Key(key) = event {
				if key_match(key, self.key_config.keys.exit_popup) {
					self.hide();
				} else if key_match(
					key,
					self.key_config.keys.toggle_workarea,
				) {
					self.focused = match self.focused {
						Focus::Input => Focus::List,
						Focus::List => Focus::Input,
					};
					self.find_text
						.set_selected(self.focused == Focus::Input);
				} else if key_match(key, self.key_config.keys.enter) {
					self.finish_selection();
					self.hide();
				} else {
					match self.focused {
						Focus::Input => {
							if self
								.find_text
								.event(event)?
								.is_consumed()
							{
								self.update_query();
							}
						}
						Focus::List => {
							self.list_event(key)?;
						}
					}
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
		self.response_queue = None;
		self.visible = false;
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		Ok(())
	}
}
