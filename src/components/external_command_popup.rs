use std::{cell::RefCell, process::Command};

use super::{
	utils::string_width_align, visibility_blocking, CommandBlocking,
	CommandInfo, Component, DrawableComponent, EventState,
	TextInputComponent,
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
	text::{Span, Spans},
	widgets::{Block, Borders, Clear /*, Paragraph*/},
	Frame,
};

use crate::async_jobs::{
	AsyncDynJob, AsyncJobFeedback, BoxFeedback, JobFeedbackSender,
	JobSender,
};
//use scopeguard::defer;
//use std::io;

type CmdResult = Result<std::process::Output, std::io::Error>;

struct AsyncJobExternCmd {
	cmd: String,
}

impl AsyncJobExternCmd {
	pub fn new(cmd: String) -> Self {
		Self { cmd }
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

impl AsyncDynJob for AsyncJobExternCmd {
	fn run(
		&mut self,
		_sender: JobFeedbackSender,
	) -> Option<BoxFeedback> {
		Some(Box::new(AsyncJobExternCmdFeedback::new(
			self.do_exec_command(&self.cmd),
		)))
	}

	fn should_stop(&self) -> bool {
		false
	}
}

struct AsyncJobExternCmdFeedback {
	res: CmdResult,
}

impl AsyncJobExternCmdFeedback {
	pub fn new(res: CmdResult) -> Self {
		Self { res }
	}
}

impl AsyncJobFeedback for AsyncJobExternCmdFeedback {
	fn visit(&mut self, app: &mut crate::app::App) {
		app.external_command_popup.finish_pending_command(&self.res);
	}
}

#[derive(PartialEq)]
enum Focused {
	Input,
	List,
}

#[derive(PartialEq)]
enum ShortcutState {
	Idle,
	Assign,
}

pub struct ExternalCommandPopupComponent {
	visible: bool,
	cmdline: TextInputComponent,
	key_config: SharedKeyConfig,
	theme: SharedTheme,
	queue: Queue,
	options: SharedOptions,
	async_job_sender: JobSender,

	selected_idx: usize,
	visible_idx: RefCell<usize>,
	focused: Focused,

	cmd_pending: bool,
	shortcut_state: ShortcutState,
}

impl ExternalCommandPopupComponent {
	///
	pub fn new(
		theme: SharedTheme,
		key_config: SharedKeyConfig,
		queue: Queue,
		options: SharedOptions,
		async_job_sender: JobSender,
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
			cmd_pending: false,
			async_job_sender,
			shortcut_state: ShortcutState::Idle,
		}
	}

	pub fn any_work_pending(&self) -> bool {
		self.cmd_pending
	}

	pub fn finish_pending_command(&mut self, res: &CmdResult) {
		self.cmd_pending = false;
		self.post_run_command_ui(res);
	}

	fn post_run_command_ui(&self, _res: &CmdResult) {
		if let Err(e) = _res {
			self.queue.push(
				crate::queue::InternalEvent::ShowErrorMsg(format!(
					"{}\n{}",
					"Command failed", e
				)),
			);
		} else {
			let o = _res.as_ref().unwrap();
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

	fn run_command_ui(&mut self, cmd: String) {
		self.cmd_pending = true;
		self.options.borrow_mut().add_extern_command(cmd.as_str());
		if let Err(_) = self
			.async_job_sender
			.send(Box::new(AsyncJobExternCmd::new(cmd)))
		{
			self.cmd_pending = false;
			self.post_run_command_ui(&Err(std::io::Error::from(
				std::io::ErrorKind::Other,
			)));
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
					let s = if i.1 .0.len() <= w.into() {
						&i.1 .0
					} else {
						&i.1 .0[0..w.into()]
					};
					let selected = if self.focused == Focused::List
						&& i.0 == self.selected_idx
					{
						true
					} else {
						false
					};

					const KEY_WIDTH: usize = 4;
					if let Some(shortcut) = i.1 .1 {
						Spans::from(vec![
							Span::styled(
								string_width_align(
									&self
										.key_config
										.get_hint(shortcut),
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
			let opts = self.options.borrow();
			let consumed = if let Event::Key(key) = &event {
				if key_match(key, self.key_config.keys.exit_popup) {
					drop(opts);
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
						let cmdstr = opts.extern_commands()
							[self.selected_idx]
							.clone();
						drop(opts);
						self.run_command_ui(cmdstr.0.to_string());
					} else {
						drop(opts);
						self.run_command_ui(
							self.cmdline.get_text().to_string(),
						);
					}
					self.hide();
					true
				} else if self.focused == Focused::List
					&& !opts.extern_commands().is_empty()
				{
					if self.shortcut_state == ShortcutState::Assign {
						drop(opts);
						let mut opts = self.options.borrow_mut();
						self.shortcut_state = ShortcutState::Idle;
						opts.assign_shortcut_for_extern_command(
							self.selected_idx,
							Some(key.into()),
						);
						true
					} else if key_match(
						key,
						self.key_config.keys.move_down,
					) {
						if self.selected_idx
							< opts.extern_commands().len() - 1
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
						drop(opts);
						self.selected_idx = self
							.options
							.borrow_mut()
							.remove_extern_command(self.selected_idx);
						true
					} else if key_match(
						key,
						self.key_config.keys.assign_shortcut,
					) {
						self.shortcut_state = ShortcutState::Assign;
						true
					} else if key_match(
						key,
						self.key_config.keys.clear_shortcut,
					) {
						drop(opts);
						self.options
							.borrow_mut()
							.assign_shortcut_for_extern_command(
								self.selected_idx,
								None,
							);
						true
					} else if key_match(
						key,
						self.key_config.keys.clear_all_shortcut,
					) {
						drop(opts);
						self.options
							.borrow_mut()
							.clear_all_shortcuts_for_extern_commands(
							);
						true
					} else if let Some(cmd) =
						opts.find_extern_cmd_for_shortcut(key.into())
					{
						drop(opts);
						self.run_command_ui(cmd);
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
		self.focused =
			if self.options.borrow().extern_commands().is_empty() {
				Focused::Input
			} else {
				Focused::List
			};
		self.cmdline.clear();
		self.cmdline.show()?;

		Ok(())
	}
}
