use super::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, TextInputComponent,
};
use crate::{
	components::utils::string_width_align,
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::{InternalEvent, Queue},
	strings::{self},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use asyncgit::sync::ShowUntrackedFilesConfig;
use crossterm::event::Event;
use ratatui::{
	backend::Backend,
	layout::{Alignment, Constraint, Direction, Layout, Rect},
	style::{Modifier, Style},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear, Paragraph, Tabs},
	Frame,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppOption {
	StatusShowUntracked,
	DiffIgnoreWhitespaces,
	DiffContextLines,
	DiffInterhunkLines,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TabType {
	Misc,
	GitCmds,
}

impl TabType {
	pub fn next(self) -> Self {
		match self {
			TabType::Misc => TabType::GitCmds,
			TabType::GitCmds => TabType::Misc,
		}
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GitCmdOption {
	GitPush,
	GitFetch,
	GitCheckout,
}

impl GitCmdOption {
	pub fn next(&mut self) {
		*self = match self {
			GitCmdOption::GitPush => GitCmdOption::GitFetch,
			GitCmdOption::GitFetch => GitCmdOption::GitCheckout,
			GitCmdOption::GitCheckout => GitCmdOption::GitPush,
		}
	}

	pub fn prev(&mut self) {
		*self = match self {
			GitCmdOption::GitPush => GitCmdOption::GitCheckout,
			GitCmdOption::GitCheckout => GitCmdOption::GitFetch,
			GitCmdOption::GitFetch => GitCmdOption::GitPush,
		}
	}
}

pub struct OptionsPopupComponent {
	selection: AppOption,
	queue: Queue,
	visible: bool,
	key_config: SharedKeyConfig,
	options: SharedOptions,
	theme: SharedTheme,
	current_tab: TabType,
	git_cmd_selection: GitCmdOption,

	input_git_push: TextInputComponent,
	input_git_fetch: TextInputComponent,
	input_git_checkout: TextInputComponent,

	git_cmd_editing: bool,
}

impl OptionsPopupComponent {
	///
	pub fn new(
		queue: &Queue,
		theme: SharedTheme,
		key_config: SharedKeyConfig,
		options: SharedOptions,
	) -> Self {
		Self {
			selection: AppOption::StatusShowUntracked,
			queue: queue.clone(),
			visible: false,
			options,
			current_tab: TabType::Misc,
			git_cmd_selection: GitCmdOption::GitPush,
			input_git_push: TextInputComponent::new(
				theme.clone(),
				key_config.clone(),
				"",
				"git push command here",
				false,
			)
			.with_input_type(super::InputType::Singleline)
			.make_embed()
			.make_visible(),

			input_git_fetch: TextInputComponent::new(
				theme.clone(),
				key_config.clone(),
				"",
				"git fetch command here",
				false,
			)
			.with_input_type(super::InputType::Singleline)
			.make_embed()
			.make_visible(),

			input_git_checkout: TextInputComponent::new(
				theme.clone(),
				key_config.clone(),
				"",
				"git checkout command here",
				false,
			)
			.with_input_type(super::InputType::Singleline)
			.make_embed()
			.make_visible(),

			key_config,
			theme,
			git_cmd_editing: false,
		}
	}

	fn get_text(&self, width: u16) -> Vec<Spans> {
		let mut txt: Vec<Spans> = Vec::with_capacity(10);

		self.add_status(&mut txt, width);

		txt
	}

	fn add_status(&self, txt: &mut Vec<Spans>, width: u16) {
		Self::add_header(txt, "Status");

		self.add_entry(
			txt,
			width,
			"Show untracked",
			match self.options.borrow().status_show_untracked() {
				None => "Gitconfig",
				Some(ShowUntrackedFilesConfig::No) => "No",
				Some(ShowUntrackedFilesConfig::Normal) => "Normal",
				Some(ShowUntrackedFilesConfig::All) => "All",
			},
			self.is_select(AppOption::StatusShowUntracked),
		);
		Self::add_header(txt, "");

		let diff = self.options.borrow().diff_options();
		Self::add_header(txt, "Diff");
		self.add_entry(
			txt,
			width,
			"Ignore whitespaces",
			&diff.ignore_whitespace.to_string(),
			self.is_select(AppOption::DiffIgnoreWhitespaces),
		);
		self.add_entry(
			txt,
			width,
			"Context lines",
			&diff.context.to_string(),
			self.is_select(AppOption::DiffContextLines),
		);
		self.add_entry(
			txt,
			width,
			"Inter hunk lines",
			&diff.interhunk_lines.to_string(),
			self.is_select(AppOption::DiffInterhunkLines),
		);
	}

	fn is_select(&self, kind: AppOption) -> bool {
		self.selection == kind
	}

	fn add_header(txt: &mut Vec<Spans>, header: &'static str) {
		txt.push(Spans::from(vec![Span::styled(
			header,
			//TODO: use style
			Style::default().add_modifier(Modifier::UNDERLINED),
		)]));
	}

	fn render_input<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
		entry: &'static str,
		field: &TextInputComponent,
		selected: bool,
	) -> Result<()> {
		let title_size = usize::from(area.width / 3);
		let input_size = area.width as usize - title_size;
		let title = Span::styled(
			string_width_align(entry, title_size),
			self.theme.text(true, selected),
		);
		let title_area = Rect {
			x: area.x,
			y: area.y,
			width: title_size as u16,
			height: area.height,
		};
		let input_area = Rect {
			x: area.x + title_size as u16,
			y: area.y,
			width: input_size as u16,
			height: area.height,
		};

		f.render_widget(Paragraph::new(title), title_area);
		field.draw(f, input_area)?;
		Ok(())
	}

	fn add_entry(
		&self,
		txt: &mut Vec<Spans>,
		width: u16,
		entry: &'static str,
		value: &str,
		selected: bool,
	) {
		let half = usize::from(width / 2);
		txt.push(Spans::from(vec![
			Span::styled(
				string_width_align(entry, half),
				self.theme.text(true, false),
			),
			Span::styled(
				format!("{value:^half$}"),
				self.theme.text(true, selected),
			),
		]));
	}

	fn render_git_cmds_tab<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		let outer_block = Block::default()
			.borders(Borders::ALL)
			.border_style(self.theme.block(true));
		let mut content_rect = outer_block.inner(area);
		f.render_widget(outer_block, area);
		content_rect.height = 1;
		self.render_input(
			f,
			content_rect,
			"Git Push",
			&self.input_git_push,
			self.git_cmd_selection == GitCmdOption::GitPush,
		)?;
		content_rect.y += 1;
		self.render_input(
			f,
			content_rect,
			"Git Fetch",
			&self.input_git_fetch,
			self.git_cmd_selection == GitCmdOption::GitFetch,
		)?;
		content_rect.y += 1;
		self.render_input(
			f,
			content_rect,
			"Git Checkout",
			&self.input_git_checkout,
			self.git_cmd_selection == GitCmdOption::GitCheckout,
		)?;
		Ok(())
	}

	fn render_misc_options_tab<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		f.render_widget(
			Paragraph::new(self.get_text(area.width))
				.block(
					Block::default()
						.borders(Borders::ALL)
						.border_style(self.theme.block(true)),
				)
				.alignment(Alignment::Left),
			area,
		);
		Ok(())
	}

	fn move_selection(&mut self, up: bool) {
		if up {
			self.selection = match self.selection {
				AppOption::StatusShowUntracked => {
					AppOption::DiffInterhunkLines
				}
				AppOption::DiffIgnoreWhitespaces => {
					AppOption::StatusShowUntracked
				}
				AppOption::DiffContextLines => {
					AppOption::DiffIgnoreWhitespaces
				}
				AppOption::DiffInterhunkLines => {
					AppOption::DiffContextLines
				}
			};
		} else {
			self.selection = match self.selection {
				AppOption::StatusShowUntracked => {
					AppOption::DiffIgnoreWhitespaces
				}
				AppOption::DiffIgnoreWhitespaces => {
					AppOption::DiffContextLines
				}
				AppOption::DiffContextLines => {
					AppOption::DiffInterhunkLines
				}
				AppOption::DiffInterhunkLines => {
					AppOption::StatusShowUntracked
				}
			};
		}
	}

	fn switch_option(&mut self, right: bool) {
		//let mut opts: std::cell::RefMut<Options> =
		//	self.options.borrow_mut();
		if right {
			match self.selection {
				AppOption::StatusShowUntracked => {
					let untracked =
						self.options.borrow().status_show_untracked();

					let untracked = match untracked {
						None => {
							Some(ShowUntrackedFilesConfig::Normal)
						}
						Some(ShowUntrackedFilesConfig::Normal) => {
							Some(ShowUntrackedFilesConfig::All)
						}
						Some(ShowUntrackedFilesConfig::All) => {
							Some(ShowUntrackedFilesConfig::No)
						}
						Some(ShowUntrackedFilesConfig::No) => None,
					};

					self.options
						.borrow_mut()
						.set_status_show_untracked(untracked);
				}
				AppOption::DiffIgnoreWhitespaces => {
					self.options
						.borrow_mut()
						.diff_toggle_whitespace();
				}
				AppOption::DiffContextLines => {
					self.options
						.borrow_mut()
						.diff_context_change(true);
				}
				AppOption::DiffInterhunkLines => {
					self.options
						.borrow_mut()
						.diff_hunk_lines_change(true);
				}
			};
		} else {
			match self.selection {
				AppOption::StatusShowUntracked => {
					let untracked =
						self.options.borrow().status_show_untracked();

					let untracked = match untracked {
						None => Some(ShowUntrackedFilesConfig::No),
						Some(ShowUntrackedFilesConfig::No) => {
							Some(ShowUntrackedFilesConfig::All)
						}
						Some(ShowUntrackedFilesConfig::All) => {
							Some(ShowUntrackedFilesConfig::Normal)
						}
						Some(ShowUntrackedFilesConfig::Normal) => {
							None
						}
					};

					self.options
						.borrow_mut()
						.set_status_show_untracked(untracked);
				}
				AppOption::DiffIgnoreWhitespaces => {
					self.options
						.borrow_mut()
						.diff_toggle_whitespace();
				}
				AppOption::DiffContextLines => {
					self.options
						.borrow_mut()
						.diff_context_change(false);
				}
				AppOption::DiffInterhunkLines => {
					self.options
						.borrow_mut()
						.diff_hunk_lines_change(false);
				}
			};
		}

		self.queue
			.push(InternalEvent::OptionSwitched(self.selection));
	}

	fn event_misc(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if let Event::Key(key) = event {
			if key_match(key, self.key_config.keys.move_up) {
				self.move_selection(true);
			} else if key_match(key, self.key_config.keys.move_down) {
				self.move_selection(false);
			} else if key_match(key, self.key_config.keys.move_right)
			{
				self.switch_option(true);
			} else if key_match(key, self.key_config.keys.move_left) {
				self.switch_option(false);
			}
		}

		return Ok(EventState::Consumed);
	}

	fn get_selected_git_input(&self) -> &TextInputComponent {
		match self.git_cmd_selection {
			GitCmdOption::GitPush => &self.input_git_push,
			GitCmdOption::GitFetch => &self.input_git_fetch,
			GitCmdOption::GitCheckout => &self.input_git_checkout,
		}
	}

	fn get_selected_git_input_mut(
		&mut self,
	) -> &mut TextInputComponent {
		match self.git_cmd_selection {
			GitCmdOption::GitPush => &mut self.input_git_push,
			GitCmdOption::GitFetch => &mut self.input_git_fetch,
			GitCmdOption::GitCheckout => &mut self.input_git_checkout,
		}
	}

	fn get_selected_git_cmd(&self) -> String {
		self.get_selected_git_input().get_text().to_string()
	}

	fn event_git_cmds(
		&mut self,
		event: &crossterm::event::Event,
	) -> Result<EventState> {
		if let Event::Key(key) = event {
			if key_match(key, self.key_config.keys.enter) {
				if self.git_cmd_editing {
					//finish editing
					self.git_cmd_editing = false;
					self.get_selected_git_input_mut()
						.set_selected(false);
					let res = self.get_selected_git_cmd();
					let res =
						if res.is_empty() { None } else { Some(res) };
					match self.git_cmd_selection {
						GitCmdOption::GitPush => self
							.options
							.borrow_mut()
							.set_git_extern_push(res),
						GitCmdOption::GitFetch => self
							.options
							.borrow_mut()
							.set_git_extern_fetch(res),
						GitCmdOption::GitCheckout => self
							.options
							.borrow_mut()
							.set_git_extern_checkout(res),
					}
				} else {
					//enter editing
					self.git_cmd_editing = true;
					self.get_selected_git_input_mut()
						.set_selected(true);
				}
			} else if self.git_cmd_editing {
				//forward
				return match self.git_cmd_selection {
					GitCmdOption::GitPush => {
						self.input_git_push.event(event)
					}
					GitCmdOption::GitFetch => {
						self.input_git_fetch.event(event)
					}
					GitCmdOption::GitCheckout => {
						self.input_git_checkout.event(event)
					}
				};
			} else if key_match(key, self.key_config.keys.move_up) {
				self.git_cmd_selection.prev();
			} else if key_match(key, self.key_config.keys.move_down) {
				self.git_cmd_selection.next();
			}
		}

		return Ok(EventState::Consumed);
	}
}

impl DrawableComponent for OptionsPopupComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		if self.is_visible() {
			const SIZE: (u16, u16) = (50, 12);
			let area =
				ui::centered_rect_absolute(SIZE.0, SIZE.1, area);
			f.render_widget(Clear, area);

			let chunks = Layout::default()
				.direction(Direction::Vertical)
				//.margin(5)
				.constraints(
					[Constraint::Length(3), Constraint::Min(0)]
						.as_ref(),
				)
				.split(area);

			let titles = ["Misc", "Git commands"]
				.iter()
				.cloned()
				.map(|i| Spans::from(i))
				.collect();

			let tabs = Tabs::new(titles)
				.block(Block::default().borders(Borders::ALL))
				.style(self.theme.tab(false))
				.highlight_style(self.theme.tab(true))
				.select(self.current_tab as usize)
				.divider("|");
			f.render_widget(tabs, chunks[0]);

			match self.current_tab {
				TabType::Misc => {
					self.render_misc_options_tab(f, chunks[1])?
				}
				TabType::GitCmds => {
					self.render_git_cmds_tab(f, chunks[1])?
				}
			}
		}

		Ok(())
	}
}

impl Component for OptionsPopupComponent {
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
					return Ok(EventState::Consumed);
				} else if key_match(
					key,
					self.key_config.keys.toggle_workarea,
				) {
					self.current_tab = self.current_tab.next();
					return Ok(EventState::Consumed);
				}
			}

			return match self.current_tab {
				TabType::Misc => self.event_misc(event),
				TabType::GitCmds => self.event_git_cmds(event),
			};
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
		self.input_git_push.set_text(
			self.options
				.borrow()
				.git_extern_commands()
				.push_base
				.as_ref()
				.map_or(String::new(), |i| i.clone()),
		);
		self.input_git_fetch.set_text(
			self.options
				.borrow()
				.git_extern_commands()
				.fetch_base
				.as_ref()
				.map_or(String::new(), |i| i.clone()),
		);
		self.input_git_checkout.set_text(
			self.options
				.borrow()
				.git_extern_commands()
				.checkout_base
				.as_ref()
				.map_or(String::new(), |i| i.clone()),
		);

		Ok(())
	}
}
