use std::{cell::RefCell, process::Command};

use super::{
	utils::string_width_align, visibility_blocking, CommandBlocking,
	CommandInfo, Component, DrawableComponent, EventState,
	TextInputComponent,
};
use crate::{
	//components::utils::string_width_align,
	keys::{key_match, GituiKeyEvent, SharedKeyConfig},
	options::SharedOptions,
	queue::Queue,
	strings::{self},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::{
	backend::Backend,
	layout::{/*Alignment,*/ Constraint, Layout, Margin, Rect},
	text::{Span, Spans},
	widgets::{Block, Borders, Clear /*, Paragraph*/},
	Frame,
};
//use scopeguard::defer;
//use std::io;

#[derive(PartialEq)]
enum Focused {
	Input,
	List,
}

pub struct ExternalCommandPopupComponent {
	visible: bool,
	cmdline: TextInputComponent,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
	queue: Queue,
	options: SharedOptions,

	selected_idx: usize,
	visible_idx: RefCell<usize>,
	focused: Focused,
}

impl ExternalCommandPopupComponent {
	///
	pub fn new(
		theme: SharedTheme,
		key_config: SharedKeyConfig,
		queue: Queue,
		options: SharedOptions,
	) -> Self {
		Self {
			visible: false,
			cmdline: TextInputComponent::new(
				theme.clone(),
				key_config.clone(),
				"",
				"Enter command here",
				false,
			)
			.with_input_type(super::InputType::Singleline)
			.make_embed(),
			key_config,
			theme,
			queue,
			options,
			selected_idx: 0,
			visible_idx: 0.into(),
			focused: Focused::Input,
		}
	}

	fn exec_command(
		&self,
		cmd: &str,
	) -> Result<std::process::Output, std::io::Error> {
		/*io::stdout().execute(LeaveAlternateScreen)?;
		defer! {
			io::stdout().execute(EnterAlternateScreen).expect("reset terminal");
		}*/
		self.options.borrow_mut().add_extern_command(cmd);
		self.do_exec_command(cmd)
	}

	#[cfg(unix)]
	fn do_exec_command(
		&self,
		cmd: &str,
	) -> Result<std::process::Output, std::io::Error> {
		Command::new("sh").args(["-c", cmd]).output()
	}

	#[cfg(windows)]
	fn do_exec_command(
		&self,
		cmd: &str,
	) -> Result<std::process::Output, std::io::Error> {
		Command::new("cmd.exe").args(["/C", cmd]).output()
	}

	fn run_command_ui(&self, cmd: &str) {
		let _res = self.exec_command(cmd);
		if let Err(e) = _res {
			self.queue.push(
				crate::queue::InternalEvent::ShowErrorMsg(format!(
					"{}\n{}",
					"Command failed", e
				)),
			);
		} else {
			let o = _res.unwrap();
			if !o.stderr.is_empty() && !o.status.success() {
				self.queue.push(
					crate::queue::InternalEvent::ShowErrorMsg(
						format!(
							"{}",
							std::str::from_utf8(o.stderr.as_slice())
								.unwrap_or_default()
						),
					),
				);
			} else {
				let out_str = if !o.stdout.is_empty() {
					o.stdout.as_slice()
				} else {
					o.stderr.as_slice()
				};
				self.queue.push(
					crate::queue::InternalEvent::ShowInfoMsg(
						format!(
							"{}",
							std::str::from_utf8(out_str)
								.unwrap_or_default()
						),
					),
				);
			}
		}
	}
}

impl DrawableComponent for ExternalCommandPopupComponent {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		if self.is_visible() {
			let sz: (u16, u16) =
				((area.width as f32 * 0.75) as u16, 10);
			let area = ui::centered_rect_absolute(sz.0, sz.1, area);

			let v_blocks = Layout::default()
				.direction(ratatui::layout::Direction::Vertical)
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

			f.render_widget(Clear, area);
			f.render_widget(
				Block::default()
					.borders(Borders::ALL)
					.title(Span::styled(
						"Run external command",
						self.theme.title(true),
					))
					.border_style(self.theme.block(true)),
				area,
			);

			self.cmdline.draw(f, v_blocks[0])?;

			let xh = v_blocks[1].height as usize - 1;
			let mut vis_idx = self.visible_idx.borrow_mut();
			if *vis_idx > self.selected_idx {
				*vis_idx = self.selected_idx;
			} else if (*vis_idx + xh) <= self.selected_idx {
				*vis_idx = self.selected_idx - xh + 1;
			}
			drop(vis_idx);
			let vis_idx = *self.visible_idx.borrow();

			let w = v_blocks[1].width;
			let opts = self.options.borrow();
			let prev_command_spans = opts
				.extern_commands()
				.iter()
				.enumerate()
				.skip(vis_idx)
				.take(xh)
				.map(|i| {
					let s = if i.1.len() <= w.into() {
						i.1
					} else {
						&i.1[0..w.into()]
					};
					let selected = if self.focused == Focused::List
						&& i.0 == self.selected_idx
					{
						true
					} else {
						false
					};

					const KEY_WIDTH: usize = 4;
					let mut relative_visible_idx = i.0 - vis_idx;
					if relative_visible_idx < 10 {
						relative_visible_idx =
							(relative_visible_idx + 1) % 10;
						let key = GituiKeyEvent::new(
							KeyCode::Char(
								('0' as u8
									+ relative_visible_idx as u8) as char,
							),
							KeyModifiers::ALT,
						);
						Spans::from(vec![
							Span::styled(
								string_width_align(
									&self.key_config.get_hint(key),
									KEY_WIDTH,
								),
								self.theme.text(true, selected),
							),
							Span::styled(
								s,
								self.theme.text(true, selected),
							),
						])
					} else {
						Spans::from(vec![
							Span::styled(
								string_width_align(" ", KEY_WIDTH),
								self.theme.text(true, selected),
							),
							Span::styled(
								s,
								self.theme.text(true, selected),
							),
						])
					}
				});

			ui::draw_list_block(
				f,
				v_blocks[1],
				Block::default()
					.title(Span::styled(
						"Prev commands",
						self.theme.title(true),
					))
					.borders(Borders::TOP),
				prev_command_spans,
			);
		}

		Ok(())
	}
}

impl Component for ExternalCommandPopupComponent {
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
				strings::commands::switch_focus(&self.key_config),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::execute_command(&self.key_config),
				true,
				true,
			));
			out.push(CommandInfo::new(
				strings::commands::delete_command(&self.key_config),
				true,
				self.focused == Focused::List,
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
					self.key_config.keys.toggle_workarea,
				) {
					self.focused = if self.focused == Focused::Input {
						Focused::List
					} else {
						Focused::Input
					};
					true
				} else if key_match(key, self.key_config.keys.enter) {
					if self.focused == Focused::List {
						let cmdstr =
							self.options.borrow().extern_commands()
								[self.selected_idx]
								.clone();
						self.run_command_ui(&cmdstr);
					} else {
						self.run_command_ui(&self.cmdline.get_text());
					}
					self.hide();
					true
				} else if self.focused == Focused::List
					&& !self
						.options
						.borrow()
						.extern_commands()
						.is_empty()
				{
					if key_match(key, self.key_config.keys.move_down)
					{
						if self.selected_idx
							< self
								.options
								.borrow()
								.extern_commands()
								.len() - 1
						{
							self.selected_idx += 1;
						}
						true
					} else if key_match(
						key,
						self.key_config.keys.move_up,
					) {
						if self.selected_idx > 0 {
							self.selected_idx -= 1;
						}
						true
					} else if key_match(
						key,
						self.key_config.keys.delete_generic,
					) {
						self.selected_idx = self
							.options
							.borrow_mut()
							.remove_extern_command(self.selected_idx);
						true
					} else {
						false
					}
				} else if let KeyCode::Char(c) = key.code {
					if c >= '0'
						&& c <= '9' && key.modifiers
						== KeyModifiers::ALT
					{
						//run command
						let visible_offset =
							((c as u8 - '0' as u8) + 9) % 10;
						let opts = self.options.borrow();
						let extern_commands = opts.extern_commands();
						let cmd_idx = *self.visible_idx.borrow()
							+ visible_offset as usize;
						if cmd_idx < extern_commands.len() {
							let cmdstr =
								extern_commands[cmd_idx].clone();
							drop(opts);
							self.run_command_ui(&cmdstr);
							self.hide();
							true
						} else {
							false
						}
					} else {
						false
					}
				} else {
					false
				}
			} else {
				false
			};

			if !consumed && self.cmdline.event(event)?.is_consumed() {
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
		self.cmdline.hide();
	}

	fn show(&mut self) -> Result<()> {
		self.visible = true;
		self.focused = Focused::Input;
		self.cmdline.clear();
		self.cmdline.show()?;

		Ok(())
	}
}
