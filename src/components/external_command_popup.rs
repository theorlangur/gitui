use std::{
	cell::RefCell, os::unix::prelude::OsStrExt, process::Command,
};

use super::{
	visibility_blocking, CommandBlocking, CommandInfo, Component,
	DrawableComponent, EventState, TextInputComponent,
};
use crate::{
	//components::utils::string_width_align,
	keys::{key_match, SharedKeyConfig},
	options::SharedOptions,
	queue::Queue,
	strings::{self},
	ui::{self, style::SharedTheme},
};
use anyhow::Result;
use crossterm::event::Event;
use ratatui::{
	backend::Backend,
	layout::{/*Alignment,*/ Constraint, Layout, Margin, Rect},
	text::Span,
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
	) -> Result<std::process::Output, std::io::Error> {
		/*io::stdout().execute(LeaveAlternateScreen)?;
		defer! {
			io::stdout().execute(EnterAlternateScreen).expect("reset terminal");
		}*/
		let cmd = self.cmdline.get_text();
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

			let w = v_blocks[1].width;
			let opts = self.options.borrow();
			let prev_command_spans = opts
				.extern_commands()
				.iter()
				.enumerate()
				.skip(*vis_idx)
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
					Span::styled(s, self.theme.text(true, selected))
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
						self.cmdline.set_text(
							self.options.borrow().extern_commands()
								[self.selected_idx]
								.clone(),
						);
					}
					let _res = self.exec_command();
					if let Err(e) = _res {
						self.queue.push(
							crate::queue::InternalEvent::ShowErrorMsg(
								format!(
									"{}\n{}",
									"Command failed", e
								),
							),
						);
					} else {
						let o = _res.unwrap();
						if !o.stderr.is_empty() {
							self.queue.push(
                                crate::queue::InternalEvent::ShowErrorMsg(
                                    format!(
                                        "{}",
										std::ffi::OsStr::from_bytes(o.stderr.as_slice()).to_str().unwrap_or_default()
                                    ),
                                ),
                            );
						} else {
							self.queue.push(
                                crate::queue::InternalEvent::ShowInfoMsg(
                                    format!(
                                        "{}",
										std::ffi::OsStr::from_bytes(o.stdout.as_slice()).to_str().unwrap_or_default()
                                    ),
                                ),
                            );
						}
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
