use super::{
	utils::scroll_horizontal::HorizontalScroll,
	utils::scroll_vertical::VerticalScroll, CommandBlocking,
	Direction, DrawableComponent, HorizontalScrollType, ScrollType,
};
use crate::{
	components::{CommandInfo, Component, EventState},
	keys::{key_match, SharedKeyConfig},
	queue::{Action, InternalEvent, NeedsUpdate, Queue, ResetItem},
	string_utils::tabs_to_spaces,
	string_utils::trim_offset,
	strings, try_or_popup,
	ui::style::SharedTheme,
};
use anyhow::Result;
use asyncgit::{
	hash,
	sync::{self, diff::DiffLinePosition, RepoPathRef},
	DiffLine, DiffLineType, FileDiff,
};
use bytesize::ByteSize;
use crossterm::event::Event;
use crossterm::event::KeyEvent;
use crossterm::event::KeyCode;
use ratatui::{
	backend::Backend,
	layout::Rect,
	symbols,
	text::{Span, Spans},
	widgets::{Block, Borders, Paragraph},
	Frame,
};
use std::{borrow::Cow, cell::Cell, cmp, path::Path};
use std::time::SystemTime;

#[derive(Default)]
struct Current {
	path: String,
	is_stage: bool,
	hash: u64,
}

///
#[derive(Clone, Copy)]
enum Selection {
	Single(usize),
	Multiple(usize, usize),
}

impl Selection {
	const fn get_start(&self) -> usize {
		match self {
			Self::Single(start) | Self::Multiple(start, _) => *start,
		}
	}

	const fn get_end(&self) -> usize {
		match self {
			Self::Single(end) | Self::Multiple(_, end) => *end,
		}
	}

	fn get_top(&self) -> usize {
		match self {
			Self::Single(start) => *start,
			Self::Multiple(start, end) => cmp::min(*start, *end),
		}
	}

	fn get_bottom(&self) -> usize {
		match self {
			Self::Single(start) => *start,
			Self::Multiple(start, end) => cmp::max(*start, *end),
		}
	}

	fn modify(&mut self, direction: Direction, max: usize) {
		let start = self.get_start();
		let old_end = self.get_end();

		*self = match direction {
			Direction::Up => {
				Self::Multiple(start, old_end.saturating_sub(1))
			}

			Direction::Down => {
				Self::Multiple(start, cmp::min(old_end + 1, max))
			}
		};
	}

	fn contains(&self, index: usize) -> bool {
		match self {
			Self::Single(start) => index == *start,
			Self::Multiple(start, end) => {
				if start <= end {
					*start <= index && index <= *end
				} else {
					*end <= index && index <= *start
				}
			}
		}
	}
}

enum CopyState
{
	None,
	Pending,
	Size(isize),
	LinesUp(isize),
	LinesDown(isize),
	Line,
	Hunk
}

enum SearchState
{
	IncSearch(String, usize),
	Search(String)
}

enum SearchDirection
{
	Forward,
	Backward
}

struct Search
{
	pub search: Option<SearchState>,
	pub direction: SearchDirection,
	pub smart_case: bool,
	pub start_line: usize
}

impl Search{
	pub fn is_active(&self) -> bool { self.search.is_some() }
	pub fn find_in_str(&self, line: &str) -> bool {
		if self.smart_case {
			match self.search.as_ref().unwrap() {
				SearchState::IncSearch(s, _) => line.to_lowercase().find(&s.to_lowercase()).is_some(),
				SearchState::Search(s) => line.to_lowercase().find(&s.to_lowercase()).is_some(),
			}
		}else{
			match self.search.as_ref().unwrap() {
				SearchState::IncSearch(s, _) => line.find(s).is_some(),
				SearchState::Search(s) => line.find(s).is_some(),
			}
		}
	}
}

///
pub struct DiffComponent {
	repo: RepoPathRef,
	diff: Option<FileDiff>,
	longest_line: usize,
	pending: bool,
	selection: Selection,
	selected_hunk: Option<usize>,
	current_size: Cell<(u16, u16)>,
	focused: bool,
	current: Current,
	vertical_scroll: VerticalScroll,
	horizontal_scroll: HorizontalScroll,
	queue: Queue,
	theme: SharedTheme,
	key_config: SharedKeyConfig,
	is_immutable: bool,
	copy_op: CopyState,
	copied_region: Option<(Selection, SystemTime)>,
	pending_movement: Option<usize>,
	search: Search
}

impl DiffComponent {
	///
	pub fn new(
		repo: RepoPathRef,
		queue: Queue,
		theme: SharedTheme,
		key_config: SharedKeyConfig,
		is_immutable: bool,
	) -> Self {
		Self {
			focused: false,
			queue,
			current: Current::default(),
			pending: false,
			selected_hunk: None,
			diff: None,
			longest_line: 0,
			current_size: Cell::new((0, 0)),
			selection: Selection::Single(0),
			vertical_scroll: VerticalScroll::new(),
			horizontal_scroll: HorizontalScroll::new(),
			theme,
			key_config,
			is_immutable,
			repo,
			copy_op: CopyState::None,
			copied_region: None,
			pending_movement: None,
			search: Search{search: None, direction: SearchDirection::Forward, smart_case: true, start_line: 0}
		}
	}
	///
	fn can_scroll(&self) -> bool {
		self.diff
			.as_ref()
			.map(|diff| diff.lines > 1)
			.unwrap_or_default()
	}
	///
	pub fn current(&self) -> (String, bool) {
		(self.current.path.clone(), self.current.is_stage)
	}
	///
	pub fn clear(&mut self, pending: bool) {
		self.current = Current::default();
		self.diff = None;
		self.longest_line = 0;
		self.vertical_scroll.reset();
		self.horizontal_scroll.reset();
		self.selection = Selection::Single(0);
		self.selected_hunk = None;
		self.pending = pending;
	}

	pub fn on_tick(&mut self)
	{ 
		if let Some((copied, start)) = self.copied_region {
			let n = SystemTime::now();
			if n.duration_since(start).unwrap().as_millis() >= 90 {
				self.copied_region = None;
			}else
			{
				self.copied_region = Some((copied, start));
				self.queue_update();
			}
		}
	}

	///
	pub fn update(
		&mut self,
		path: String,
		is_stage: bool,
		diff: FileDiff,
	) {
		self.pending = false;

		let hash = hash(&diff);

		if self.current.hash != hash {
			let reset_selection = self.current.path != path;

			self.current = Current {
				path,
				is_stage,
				hash,
			};

			self.diff = Some(diff);

			self.longest_line = self
				.diff
				.iter()
				.flat_map(|diff| diff.hunks.iter())
				.flat_map(|hunk| hunk.lines.iter())
				.map(|line| {
					let converted_content = tabs_to_spaces(
						line.content.as_ref().to_string(),
					);

					converted_content.len()
				})
				.max()
				.map_or(0, |len| {
					// Each hunk uses a 1-character wide vertical bar to its left to indicate
					// selection.
					len + 1
				});

			if reset_selection {
				self.vertical_scroll.reset();
				self.selection = Selection::Single(0);
				self.update_selection(0);
			} else {
				let old_selection = match self.selection {
					Selection::Single(line) => line,
					Selection::Multiple(start, _) => start,
				};
				self.update_selection(old_selection);
			}
		}
	}

	fn move_hunk_selection(&mut self, move_type: ScrollType) {
		if let Some(diff) = &self.diff {
			let new_start = match move_type {
				ScrollType::Down => {
					if let Some((_,to)) = self.get_selected_hunk_line_range() {
						to
					}else{
						0
					}
				}
				ScrollType::Up => {
					if let Some(hunk_index) = self.selected_hunk {
						if let Some((from,_to)) = Self::get_hunk_line_range(diff, hunk_index.saturating_sub(1)) {
							from
						}else {
							0
						}
					}else{
						0
					}
				}
				_ => self.selection.get_start()
			};

			self.update_selection(new_start);
		}
	}

	fn move_selection(&mut self, move_type: ScrollType) {
		if let Some(diff) = &self.diff {
			let max = diff.lines.saturating_sub(1);

			let new_start = match move_type {
				ScrollType::Down => {
					self.selection.get_bottom().saturating_add(1)
				}
				ScrollType::Up => {
					self.selection.get_top().saturating_sub(1)
				}
				ScrollType::Home => 0,
				ScrollType::End => max,
				ScrollType::PageDown => {
					self.selection.get_bottom().saturating_add(
						self.current_size.get().1.saturating_sub(1)
							as usize,
					)
				}
				ScrollType::PageUp => {
					self.selection.get_top().saturating_sub(
						self.current_size.get().1.saturating_sub(1)
							as usize,
					)
				}
			};

			self.update_selection(new_start);
		}
	}

	fn update_selection(&mut self, new_start: usize) {
		if let Some(diff) = &self.diff {
			let max = diff.lines.saturating_sub(1);
			let new_start = cmp::min(max, new_start);
			self.selection = Selection::Single(new_start);
			self.selected_hunk =
				Self::find_selected_hunk(diff, new_start);
		}
	}

	fn lines_count(&self) -> usize {
		self.diff.as_ref().map_or(0, |diff| diff.lines)
	}

	fn max_scroll_right(&self) -> usize {
		self.longest_line
			.saturating_sub(self.current_size.get().0.into())
	}

	fn modify_selection(&mut self, direction: Direction) {
		if self.diff.is_some() {
			self.selection.modify(direction, self.lines_count());
		}
	}

	fn copy_selection(&self) {
		if let Some(diff) = &self.diff {
			let lines_to_copy: Vec<&str> =
				diff.hunks
					.iter()
					.flat_map(|hunk| hunk.lines.iter())
					.enumerate()
					.filter_map(|(i, line)| {
						if self.selection.contains(i) {
							Some(line.content.trim_matches(|c| {
								c == '\n' || c == '\r'
							}))
						} else {
							None
						}
					})
					.collect();

			try_or_popup!(
				self,
				"copy to clipboard error:",
				crate::clipboard::copy_string(
					&lines_to_copy.join("\n")
				)
			);
		}
	}

	fn search_forward(&mut self, start: Option<usize>)
	{
		let start_index = start.unwrap_or(self.selection.get_start());
		let line_num = self
			.diff
			.iter()
			.flat_map(|diff| diff.hunks.iter())
			.flat_map(|hunk| hunk.lines.iter())
			.enumerate()
			.skip(start_index + 1)
			.find(|(_idx, line)|{
				self.search.find_in_str(&*line.content)
			})
		.map_or(start_index, |(idx, _line)| { idx });
		self.update_selection(line_num);
	}

	fn search_backwards(&mut self, start: Option<usize>)
	{
		let start_index = start.unwrap_or(self.selection.get_start());
		let line_num = self
			.diff
			.iter()
			.flat_map(|diff| diff.hunks.iter())
			.flat_map(|hunk| hunk.lines.iter())
			.enumerate()
			.take(start_index)
			.filter(|(_idx, line)|{
				self.search.find_in_str(&*line.content)
			})
		.last()
			.map_or(start_index, |(idx, _line)| { idx });
		self.update_selection(line_num);
	}

	fn search_event(&mut self, e: &KeyEvent) -> Result<EventState> {
		if key_match(e, self.key_config.keys.enter) { 
			self.search.search = match &self.search.search {
				Some(SearchState::IncSearch(s, _)) => if !s.is_empty() { Some(SearchState::Search(s.to_string())) } else { None },
				Some(SearchState::Search(s)) => Some(SearchState::Search(s.to_string())),
				None => None
			};
			return Ok(EventState::Consumed);
		}else if key_match(e, self.key_config.keys.exit_popup)
		{
			let was_active = self.search.is_active();
			if let Some(SearchState::IncSearch(_,p)) = self.search.search.take() {
				self.update_selection(p);
				return Ok(EventState::Consumed);
			}
			return if was_active { Ok(EventState::Consumed) } else { Ok(EventState::NotConsumed) };
		} 
		if let Some(SearchState::Search(_s)) = &self.search.search {
			if key_match(e, self.key_config.keys.search_next) { 
				match self.search.direction {
					SearchDirection::Forward => self.search_forward(None),
					SearchDirection::Backward => self.search_backwards(None),
				}
				return Ok(EventState::Consumed);
			}else if key_match(e, self.key_config.keys.search_prev) { 
				match self.search.direction {
					SearchDirection::Backward => self.search_forward(None),
					SearchDirection::Forward => self.search_backwards(None),
				}
				return Ok(EventState::Consumed);
			}
			return Ok(EventState::NotConsumed);
		}else if let Some(SearchState::IncSearch(s, orig_pos)) = &mut self.search.search {
			if let KeyCode::Char(c) = e.code {
				if !c.is_control() {
					if c.is_uppercase() {
						self.search.smart_case = false;
					}
					let cs = c.to_string();
					*s += &cs;
					let opos = *orig_pos;
					match self.search.direction {
						SearchDirection::Forward => self.search_forward(Some(opos)),
						SearchDirection::Backward => self.search_backwards(Some(opos)),
					}
				}
				return Ok(EventState::Consumed);
			}else if let KeyCode::Backspace = e.code {
				s.remove(s.len() - 1);
				let opos = *orig_pos;
				match self.search.direction {
					SearchDirection::Forward => self.search_forward(Some(opos)),
					SearchDirection::Backward => self.search_backwards(Some(opos)),
				}
			}
		}
		return Ok(EventState::NotConsumed);
	}

	fn movement_event(&mut self, e: &KeyEvent) -> Result<EventState> {
		if key_match(e, self.key_config.keys.move_up) { 
			if let Some(s) = self.pending_movement {
				self.update_selection(self.selection.get_start().saturating_sub(s));
				self.pending_movement = None;
				return Ok(EventState::Consumed);
			}
		}else if key_match(e, self.key_config.keys.move_down) { 
			if let Some(s) = self.pending_movement {
				self.update_selection(self.selection.get_start().saturating_add(s).min(self.lines_count() - 1));
				self.pending_movement = None;
				return Ok(EventState::Consumed);
			}
		}else if let KeyCode::Char(c) = e.code {
			self.pending_movement = if let Some(d) = c.to_digit(10) {
				let d : usize = d.try_into().unwrap();
				match self.pending_movement {
					None => Some(d),
					Some(s) => Some(s * 10 + d)
				}
			}else {
				None
			};

			if self.pending_movement.is_some() {
				return Ok(EventState::Consumed);
			}
		}else
		{
			self.pending_movement = None;
		}
		Ok(EventState::NotConsumed)
	}

	fn copy_event(&mut self, e: &KeyEvent) -> Result<EventState> {
		if key_match(e, self.key_config.keys.copy) {
			self.copy_op  = match self.copy_op {
				CopyState::Pending => CopyState::Line,
				_ => CopyState::None
			};
		}else if key_match(e, self.key_config.keys.copy_hunk) {
			self.copy_op  = match self.copy_op {
				CopyState::Pending => CopyState::Hunk,
				_ => CopyState::None
			};
		}else if key_match(e, self.key_config.keys.move_up) { 
			self.copy_op  = match self.copy_op {
				CopyState::Pending => CopyState::LinesUp(1),
				CopyState::Size(s) => CopyState::LinesUp(s),
				_ => CopyState::None
			};
		}else if key_match(e, self.key_config.keys.move_down) { 
			self.copy_op  = match self.copy_op {
				CopyState::Pending => CopyState::LinesDown(1),
				CopyState::Size(s) => CopyState::LinesDown(s),
				_ => CopyState::None
			};
		}else if let KeyCode::Char(c) = e.code {
			self.copy_op = if let Some(d) = c.to_digit(10) {
				let d : isize = d.try_into().unwrap();
				match self.copy_op {
					CopyState::Pending => CopyState::Size(d),
					CopyState::Size(s) => CopyState::Size(s * 10 + d),
					_ => CopyState::None
				}
			}else {
				CopyState::None
			};
		}else
		{
			self.copy_op = CopyState::None;
		}

		let orig_start = self.selection.get_start();
		//try execute
		match self.copy_op {
			CopyState::Line => {
				self.copied_region = Some((self.selection, SystemTime::now())).into();
				self.copy_selection();
				self.copy_op = CopyState::None;
			},
			CopyState::Hunk => {
				if let Some(hr) = self.get_selected_hunk_line_range() {
					//place selection at the start of the hunk
					self.update_selection(hr.0);
					//expand to the whole size of the hunk
					self.selection = Selection::Multiple(hr.0, hr.1);
					self.copied_region = Some((self.selection, SystemTime::now())).into();
					//copy
					self.copy_selection();
					//place selection at the end of the hunk
					self.update_selection(orig_start);
				}
				self.copy_op = CopyState::None;
			},
			CopyState::LinesUp(s) => {
				let start = self.selection.get_start();
				self.selection = Selection::Multiple(start, start.saturating_sub(s.try_into().unwrap()));
				self.copied_region = Some((self.selection, SystemTime::now())).into();
				self.copy_selection();
				self.update_selection(orig_start);
				self.copy_op = CopyState::None;
			},
			CopyState::LinesDown(s) => {
				let start = self.selection.get_start();
				let n_lines = self.lines_count();
				self.selection = Selection::Multiple(start, cmp::min(start.saturating_add(s.try_into().unwrap()), n_lines));
				self.copied_region = Some((self.selection, SystemTime::now())).into();
				self.copy_selection();
				self.update_selection(orig_start);
				self.copy_op = CopyState::None;
			},
			_ => ()
		};

		Ok(EventState::Consumed)
	}

	fn find_selected_hunk(
		diff: &FileDiff,
		line_selected: usize,
	) -> Option<usize> {
		let mut line_cursor = 0_usize;
		for (i, hunk) in diff.hunks.iter().enumerate() {
			let hunk_len = hunk.lines.len();
			let hunk_min = line_cursor;
			let hunk_max = line_cursor + hunk_len;

			let hunk_selected =
				hunk_min <= line_selected && hunk_max > line_selected;

			if hunk_selected {
				return Some(i);
			}

			line_cursor += hunk_len;
		}

		None
	}

	fn get_hunk_line_range(
		diff: &FileDiff,
		hunk_index: usize,
	) -> Option<(usize,usize)> {
		let mut line_cursor = 0_usize;
		for (i, hunk) in diff.hunks.iter().enumerate() {
			let hunk_len = hunk.lines.len();
			let hunk_min = line_cursor;
			let hunk_max = line_cursor + hunk_len;

			if hunk_index == i {
				return Some((hunk_min, hunk_max))
			}
			line_cursor += hunk_len;
		}

		None
	}

	fn get_selected_hunk_line_range(&self) -> Option<(usize,usize)> {
		if let Some(h) = self.selected_hunk.as_ref() {
			return Self::get_hunk_line_range(
				self.diff.as_ref().unwrap(),
				*h);
		}
		None
	}

	fn get_text(&self, width: u16, height: u16) -> Vec<Spans> {
		let mut res: Vec<Spans> = Vec::new();
		if let Some(diff) = &self.diff {
			if diff.hunks.is_empty() {
				let is_positive = diff.size_delta >= 0;
				let delta_byte_size =
					ByteSize::b(diff.size_delta.unsigned_abs());
				let sign = if is_positive { "+" } else { "-" };
				res.extend(vec![Spans::from(vec![
					Span::raw(Cow::from("size: ")),
					Span::styled(
						Cow::from(format!(
							"{}",
							ByteSize::b(diff.sizes.0)
						)),
						self.theme.text(false, false),
					),
					Span::raw(Cow::from(" -> ")),
					Span::styled(
						Cow::from(format!(
							"{}",
							ByteSize::b(diff.sizes.1)
						)),
						self.theme.text(false, false),
					),
					Span::raw(Cow::from(" (")),
					Span::styled(
						Cow::from(format!(
							"{sign}{delta_byte_size:}"
						)),
						self.theme.diff_line(
							if is_positive {
								DiffLineType::Add
							} else {
								DiffLineType::Delete
							},
							false,
							false
						),
					),
					Span::raw(Cow::from(")")),
				])]);
			} else {
				let num_width = (self.lines_count() as f32).log10() as u16 + 1;
				let min = self.vertical_scroll.get_top();
				let max = min + height as usize;

				let mut line_cursor = 0_usize;
				let mut lines_added = 0_usize;

				for (i, hunk) in diff.hunks.iter().enumerate() {
					let hunk_selected = self.focused()
						&& self
							.selected_hunk
							.map_or(false, |s| s == i);

					if lines_added >= height as usize {
						break;
					}

					let hunk_len = hunk.lines.len();
					let hunk_min = line_cursor;
					let hunk_max = line_cursor + hunk_len;

					if Self::hunk_visible(
						hunk_min, hunk_max, min, max,
					) {
						for (i, line) in hunk.lines.iter().enumerate()
						{
							if line_cursor >= min
								&& line_cursor <= max
							{
								let &selection = if let Some(copied) = self.copied_region.as_ref() { &copied.0 } else { &self.selection };
								let copied = self.copied_region.is_some();
								let line_number = if let Selection::Single(pos) = &self.selection { 
									((line_cursor as isize) - (*pos as isize)).abs() as usize
								} else { 
									line_cursor + 1
								};

								res.push(Self::get_line_to_add(
									width - num_width,
									line,
									self.focused()
										&& selection
											.contains(line_cursor),
											copied,
									hunk_selected,
									i == hunk_len - 1,
									&self.theme,
									self.horizontal_scroll
										.get_right(),
										num_width,
										line_number
								));
								lines_added += 1;
							}

							line_cursor += 1;
						}
					} else {
						line_cursor += hunk_len;
					}
				}
			}
		}
		res
	}

	fn get_line_to_add<'a>(
		width: u16,
		line: &'a DiffLine,
		selected: bool,
		copied : bool,
		selected_hunk: bool,
		end_of_hunk: bool,
		theme: &SharedTheme,
		scrolled_right: usize,
		line_number_width: u16,
		line_index: usize
	) -> Spans<'a> {
		let style = theme.diff_hunk_marker(selected_hunk);

		let num_block = Span::styled(format!("{line_index:w$}", w = line_number_width as usize), style);

		let left_side_of_line = if end_of_hunk {
			Span::styled(Cow::from(symbols::line::BOTTOM_LEFT), style)
		} else {
			match line.line_type {
				DiffLineType::Header => Span::styled(
					Cow::from(symbols::line::TOP_LEFT),
					style,
				),
				_ => Span::styled(
					Cow::from(symbols::line::VERTICAL),
					style,
				),
			}
		};

		let content =
			tabs_to_spaces(line.content.as_ref().to_string());
		let content = trim_offset(&content, scrolled_right);

		let filled = if selected {
			// selected line
			format!("{content:w$}\n", w = width as usize)
		} else {
			// weird eof missing eol line
			format!("{content}\n")
		};

		let copied_color = selected && copied;
		Spans::from(vec![
					num_block,
			left_side_of_line,
			Span::styled(
				Cow::from(filled),
				theme.diff_line(line.line_type, selected, copied_color),
			),
		])
	}

	const fn hunk_visible(
		hunk_min: usize,
		hunk_max: usize,
		min: usize,
		max: usize,
	) -> bool {
		// full overlap
		if hunk_min <= min && hunk_max >= max {
			return true;
		}

		// partly overlap
		if (hunk_min >= min && hunk_min <= max)
			|| (hunk_max >= min && hunk_max <= max)
		{
			return true;
		}

		false
	}

	fn unstage_hunk(&mut self) -> Result<()> {
		if let Some(diff) = &self.diff {
			if let Some(hunk) = self.selected_hunk {
				let hash = diff.hunks[hunk].header_hash;
				sync::unstage_hunk(
					&self.repo.borrow(),
					&self.current.path,
					hash,
				)?;
				self.queue_update();
			}
		}

		Ok(())
	}

	fn stage_hunk(&mut self) -> Result<()> {
		if let Some(diff) = &self.diff {
			if let Some(hunk) = self.selected_hunk {
				if diff.untracked {
					sync::stage_add_file(
						&self.repo.borrow(),
						Path::new(&self.current.path),
					)?;
				} else {
					let hash = diff.hunks[hunk].header_hash;
					sync::stage_hunk(
						&self.repo.borrow(),
						&self.current.path,
						hash,
					)?;
				}

				self.queue_update();
			}
		}

		Ok(())
	}

	fn queue_update(&self) {
		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
	}

	fn reset_hunk(&self) {
		if let Some(diff) = &self.diff {
			if let Some(hunk) = self.selected_hunk {
				let hash = diff.hunks[hunk].header_hash;

				self.queue.push(InternalEvent::ConfirmAction(
					Action::ResetHunk(
						self.current.path.clone(),
						hash,
					),
				));
			}
		}
	}

	fn reset_lines(&self) {
		self.queue.push(InternalEvent::ConfirmAction(
			Action::ResetLines(
				self.current.path.clone(),
				self.selected_lines(),
			),
		));
	}

	fn stage_lines(&self) {
		if let Some(diff) = &self.diff {
			//TODO: support untracked files aswell
			if !diff.untracked {
				let selected_lines = self.selected_lines();

				try_or_popup!(
					self,
					"(un)stage lines:",
					sync::stage_lines(
						&self.repo.borrow(),
						&self.current.path,
						self.is_stage(),
						&selected_lines,
					)
				);

				self.queue_update();
			}
		}
	}

	fn selected_lines(&self) -> Vec<DiffLinePosition> {
		self.diff
			.as_ref()
			.map(|diff| {
				diff.hunks
					.iter()
					.flat_map(|hunk| hunk.lines.iter())
					.enumerate()
					.filter_map(|(i, line)| {
						let is_add_or_delete = line.line_type
							== DiffLineType::Add
							|| line.line_type == DiffLineType::Delete;
						if self.selection.contains(i)
							&& is_add_or_delete
						{
							Some(line.position)
						} else {
							None
						}
					})
					.collect()
			})
			.unwrap_or_default()
	}

	fn reset_untracked(&self) {
		self.queue.push(InternalEvent::ConfirmAction(Action::Reset(
			ResetItem {
				path: self.current.path.clone(),
				is_folder: false,
			},
		)));
	}

	fn stage_unstage_hunk(&mut self) -> Result<()> {
		if self.current.is_stage {
			self.unstage_hunk()?;
		} else {
			self.stage_hunk()?;
		}

		Ok(())
	}

	const fn is_stage(&self) -> bool {
		self.current.is_stage
	}
}

impl DrawableComponent for DiffComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		r: Rect,
	) -> Result<()> {
		self.current_size.set((
			r.width.saturating_sub(2),
			r.height.saturating_sub(2),
		));

		let current_width = self.current_size.get().0;
		let current_height = self.current_size.get().1;

		self.vertical_scroll.update(
			self.selection.get_end(),
			self.lines_count(),
			usize::from(current_height),
		);

		self.horizontal_scroll.update_no_selection(
			self.longest_line,
			current_width.into(),
		);

		let title = format!(
			"{}{}",
			strings::title_diff(&self.key_config),
			self.current.path
		);

		let txt = if self.pending {
			vec![Spans::from(vec![Span::styled(
				Cow::from(strings::loading_text(&self.key_config)),
				self.theme.text(false, false),
			)])]
		} else {
			self.get_text(r.width, current_height)
		};

		f.render_widget(
			Paragraph::new(txt).block(
				Block::default()
					.title(Span::styled(
						title.as_str(),
						self.theme.title(self.focused()),
					))
					.borders(Borders::ALL)
					.border_style(self.theme.block(self.focused())),
			),
			r,
		);

		if self.focused() {
			self.vertical_scroll.draw(f, r, &self.theme);

			if self.max_scroll_right() > 0 {
				self.horizontal_scroll.draw(f, r, &self.theme);
			}
		}

		Ok(())
	}
}

impl Component for DiffComponent {
	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		out.push(CommandInfo::new(
			strings::commands::scroll(&self.key_config),
			self.can_scroll(),
			self.focused(),
		));

		match self.copy_op  {
			CopyState::None => {
				out.push(CommandInfo::new(
					strings::commands::copy(&self.key_config),
					true,
					self.focused(),
				));
			},
			_ => {
				out.push(CommandInfo::new(
					strings::commands::copy_hunk(&self.key_config),
					true,
					self.focused(),
				));
				out.push(CommandInfo::new(
					strings::commands::copy_line(&self.key_config),
					true,
					self.focused(),
				));
				let lines = if let CopyState::Size(s) = &self.copy_op {
					*s
				}else { 2 };
				out.push(CommandInfo::new(
					strings::commands::copy_below(&self.key_config, lines as usize),
					true,
					self.focused(),
				));
				out.push(CommandInfo::new(
					strings::commands::copy_above(&self.key_config, lines as usize),
					true,
					self.focused(),
				));
				return CommandBlocking::Blocking;
			},
		}

		if self.search.is_active() {
			match self.search.search.as_ref() {
				Some(SearchState::IncSearch(s, _l)) => {
					out.push(CommandInfo::new(
						strings::commands::cancel_search_init_str(&self.key_config, s),
						true,
						self.focused(),
					));
				},
				Some(SearchState::Search(s)) => {
					out.push(CommandInfo::new(
						strings::commands::search_for_text_next(&self.key_config, s),
						true,
						self.focused(),
					));
					out.push(CommandInfo::new(
						strings::commands::search_for_text_prev(&self.key_config, s),
						true,
						self.focused(),
					));
				}
				_ => ()
			}
		}
		out.push(CommandInfo::new(
			strings::commands::start_search(&self.key_config),
			true,
			self.focused(),
		));

		out.push(
			CommandInfo::new(
				strings::commands::diff_home_end(&self.key_config),
				self.can_scroll(),
				self.focused(),
			)
			.hidden(),
		);

		if !self.is_immutable {
			out.push(CommandInfo::new(
				strings::commands::diff_hunk_remove(&self.key_config),
				self.selected_hunk.is_some(),
				self.focused() && self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_hunk_add(&self.key_config),
				self.selected_hunk.is_some(),
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_hunk_revert(&self.key_config),
				self.selected_hunk.is_some(),
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_lines_revert(
					&self.key_config,
				),
				//TODO: only if any modifications are selected
				true,
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_lines_stage(&self.key_config),
				//TODO: only if any modifications are selected
				true,
				self.focused() && !self.is_stage(),
			));
			out.push(CommandInfo::new(
				strings::commands::diff_lines_unstage(
					&self.key_config,
				),
				//TODO: only if any modifications are selected
				true,
				self.focused() && self.is_stage(),
			));
		}

		CommandBlocking::PassingOn
	}

	#[allow(clippy::cognitive_complexity)]
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.focused() {
			if let Event::Key(e) = ev {
				if self.search.is_active() {
					if let Ok(EventState::Consumed) = self.search_event(e) {
						return Ok(EventState::Consumed);
					}
				}

				match self.copy_op {
					CopyState::None => (),
					_ => return self.copy_event(e)
				}

				if self.pending_movement.is_some() {
					return self.movement_event(e);
				}

				return if key_match(e, self.key_config.keys.move_down)
				{
					self.move_selection(ScrollType::Down);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.shift_down,
				) {
					self.modify_selection(Direction::Down);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.shift_up)
				{
					self.modify_selection(Direction::Up);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.end) {
					self.move_selection(ScrollType::End);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.home) {
					self.move_selection(ScrollType::Home);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_up) {
					self.move_selection(ScrollType::Up);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_right) {
					self.move_hunk_selection(ScrollType::Down);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_left) {
					self.move_hunk_selection(ScrollType::Up);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.page_up) {
					self.move_selection(ScrollType::PageUp);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.start_search_forward_init) {
					self.search.search = Some(SearchState::IncSearch(String::new(), self.selection.get_start()));
					self.search.direction = SearchDirection::Forward;
					self.search.smart_case = true;
					self.search.start_line = self.selection.get_start();
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.start_search_backward_init) {
					self.search.search = Some(SearchState::IncSearch(String::new(), self.selection.get_start()));
					self.search.direction = SearchDirection::Backward;
					self.search.smart_case = true;
					self.search.start_line = self.selection.get_start();
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.page_down)
				{
					self.move_selection(ScrollType::PageDown);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.move_right,
				) {
					self.horizontal_scroll
						.move_right(HorizontalScrollType::Right);
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.move_left)
				{
					self.horizontal_scroll
						.move_right(HorizontalScrollType::Left);
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.stage_unstage_item,
				) && !self.is_immutable
				{
					try_or_popup!(
						self,
						"hunk error:",
						self.stage_unstage_hunk()
					);

					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.status_reset_item,
				) && !self.is_immutable
					&& !self.is_stage()
				{
					if let Some(diff) = &self.diff {
						if diff.untracked {
							self.reset_untracked();
						} else {
							self.reset_hunk();
						}
					}
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_stage_lines,
				) && !self.is_immutable
				{
					self.stage_lines();
					Ok(EventState::Consumed)
				} else if key_match(
					e,
					self.key_config.keys.diff_reset_lines,
				) && !self.is_immutable
					&& !self.is_stage()
				{
					if let Some(diff) = &self.diff {
						//TODO: reset untracked lines
						if !diff.untracked {
							self.reset_lines();
						}
					}
					Ok(EventState::Consumed)
				} else if key_match(e, self.key_config.keys.copy) {
					if let Selection::Multiple(_, _) = &self.selection {
						self.copy_selection();
						Ok(EventState::Consumed)
					}else{
						match self.copy_op {
							CopyState::None => {
								self.copy_op = CopyState::Pending;
								Ok(EventState::Consumed)
							},
							_ => self.copy_event(e)
						}
					}
				}else if let KeyCode::Char(c) = e.code {
					if let Some(_d) = c.to_digit(10) {
						return self.movement_event(e);
					}else{
						Ok(EventState::NotConsumed)
					}
				} else {
					Ok(EventState::NotConsumed)
				};
			}
		}

		Ok(EventState::NotConsumed)
	}

	fn focused(&self) -> bool {
		self.focused
	}
	fn focus(&mut self, focus: bool) {
		self.focused = focus;
	}
}
