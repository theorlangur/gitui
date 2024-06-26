#![deny(
	unused_imports,
	unused_must_use,
	dead_code,
	unstable_name_collisions,
	unused_assignments
)]
#![deny(clippy::all, clippy::perf, clippy::nursery, clippy::pedantic)]
#![deny(
	clippy::unwrap_used,
	clippy::filetype_is_file,
	clippy::cargo,
	clippy::unwrap_used,
	clippy::panic,
	clippy::match_like_matches_macro
)]
#![allow(clippy::module_name_repetitions)]
#![allow(
	clippy::multiple_crate_versions,
	clippy::bool_to_int_with_if,
	clippy::module_name_repetitions
)]
// high number of false positives on nightly (as of Oct 2022 with 1.66.0-nightly)
#![allow(clippy::missing_const_for_fn)]

//TODO:
// #![deny(clippy::expect_used)]

mod app;
mod args;
mod async_jobs;
mod bug_report;
mod clipboard;
mod cmdbar;
mod components;
mod input;
mod keys;
mod notify_mutex;
mod options;
mod popup_stack;
mod profiler;
mod queue;
mod spinner;
mod string_utils;
mod strings;
mod tabs;
mod ui;
mod version;
mod watcher;

use crate::{app::App, args::process_cmdline};
use anyhow::{bail, Result};
use app::QuitState;
use asyncgit::{
	sync::{extern_git::IPCEvents, utils::repo_work_dir, RepoPath},
	AsyncGitNotification,
};
use backtrace::Backtrace;
use crossbeam_channel::{never, tick, unbounded, Receiver, Select};
use crossterm::{
	terminal::{
		disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
		LeaveAlternateScreen,
	},
	ExecutableCommand,
};
use input::{Input, InputEvent, InputState};
use keys::KeyConfig;
use profiler::Profiler;
use ratatui::{
	backend::{Backend, CrosstermBackend},
	Terminal,
};
use scopeguard::defer;
use scopetime::scope_time;
use spinner::Spinner;
use std::{
	cell::RefCell,
	io::{self, Write},
	panic,
	path::PathBuf,
	process,
	time::{Duration, Instant},
};
use ui::style::Theme;
use watcher::RepoWatcher;

static TICK_INTERVAL_INT: u64  = 120;//ms
static UPDATE_INTERVAL_INT: u64 = 5000;//ms
static TICK_INTERVAL: Duration = Duration::from_millis(TICK_INTERVAL_INT);
static SPINNER_INTERVAL: Duration = Duration::from_millis(80);

///
#[derive(Clone)]
pub enum QueueEvent {
	Tick,
	Notify,
	SpinnerUpdate,
	AsyncEvent(AsyncNotification),
	InputEvent(InputEvent),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxHighlightProgress {
	Progress,
	Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncAppNotification {
	///
	SyntaxHighlighting(SyntaxHighlightProgress),
	///
	Notify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncNotification {
	///
	App(AsyncAppNotification),
	///
	Git(AsyncGitNotification),
}

#[derive(Clone, Copy, PartialEq)]
enum Updater {
	Ticker,
	NotifyWatcher,
}

fn dummy_mode() -> Result<()> {
	enum ArgState {
		None,
		EventId,
		Type,
	}

	let mut s = ArgState::None;
	let mut event_id = String::new();
	let mut dummy_type = String::new();
	let args = std::env::args();
	let file_path = std::env::args()
		.by_ref()
		.last()
		.ok_or(std::io::Error::from(std::io::ErrorKind::NotFound))?;
	for arg in args {
		s = match s {
			ArgState::EventId => {
				event_id = arg;
				ArgState::None
			}
			ArgState::Type => {
				dummy_type = arg;
				ArgState::None
			}
			ArgState::None => {
				if arg == "--event_id" {
					ArgState::EventId
				} else if arg == "--type" {
					ArgState::Type
				} else {
					ArgState::None
				}
			}
		}
	}

	if event_id.is_empty() {
		anyhow::bail!("Missing --event_id");
	}
	if dummy_type.is_empty() {
		anyhow::bail!("Missing --type");
	}
	let mut events = IPCEvents::connected(&event_id)?;
	events.set_str(&file_path)?;
	events.signal_connected_ready()?;

	/*std::fs::write(
		"dummy_res.txt",
		format!(
			"EventId={}\nDummyType={}\nFile={}",
			event_id, dummy_type, file_path
		),
	)?;*/

	events.wait_shutdown()?;
	Ok(())
}

fn main() -> Result<()> {
	if let Some(_) = std::env::args().find(|i| i == "--event_id") {
		/*std::fs::write(
			"dummy_begin.txt",
			format!("{}", std::env::args().join("\n")),
		)?;*/
		if let Err(e) = dummy_mode() {
			std::fs::write("dummy_err.txt", format!("{}", e))?;
			return Err(e);
		}
		return Ok(());
		//return dummy_mode();
	}

	let app_start = Instant::now();

	let cliargs = process_cmdline()?;

	let _profiler = Profiler::new();

	asyncgit::register_tracing_logging();

	if !valid_path(&cliargs.repo_path) {
		bail!("invalid path\nplease run gitui inside of a non-bare git repository");
	}

	let key_config = KeyConfig::init()
		.map_err(|e| eprintln!("KeyConfig loading error: {e}"))
		.unwrap_or_default();
	let theme = Theme::init(&cliargs.theme)
		.map_err(|e| eprintln!("Theme loading error: {e}"))
		.unwrap_or_default();

	setup_terminal()?;
	defer! {
		shutdown_terminal();
	}

	set_panic_handlers()?;

	let mut terminal = start_terminal(io::stdout())?;
	let mut repo_path = cliargs.repo_path;
	let input = Input::new();

	let updater = if cliargs.notify_watcher {
		Updater::NotifyWatcher
	} else {
		Updater::Ticker
	};

	loop {
		let quit_state = run_app(
			app_start,
			repo_path.clone(),
			theme,
			key_config.clone(),
			&input,
			updater,
			&mut terminal,
		)?;

		match quit_state {
			QuitState::OpenSubmodule(p) => {
				repo_path = p;
			}
			_ => break,
		}
	}

	Ok(())
}

static mut LFS_FILES: Vec<PathBuf> = vec![];
pub fn is_among_tracked_lfs_files(p: &str) -> bool {
	let files = unsafe { &LFS_FILES };
	files.iter().find(|i| i.starts_with(p)).is_some()
}

fn run_app(
	app_start: Instant,
	repo: RepoPath,
	theme: Theme,
	key_config: KeyConfig,
	input: &Input,
	updater: Updater,
	terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<QuitState, anyhow::Error> {
	unsafe {
		LFS_FILES = process::Command::new("git")
			.args(["lfs", "ls-files"])
			.output()
			.map_or(Vec::new(), |o| {
				std::str::from_utf8(o.stdout.as_slice())
					.unwrap_or_default()
					.split('\n')
					.skip_while(|i| i.len() == 0)
					.map(|i| {
						i.split_ascii_whitespace()
							.nth(2)
							.unwrap_or_default()
					})
					.skip_while(|i| i.len() == 0)
					.map(PathBuf::from)
					.collect()
			});
	}

	let (tx_git, rx_git) = unbounded();
	let (tx_app, rx_app) = unbounded();

	let rx_input = input.receiver();

	let (rx_ticker, rx_watcher) = match updater {
		Updater::NotifyWatcher => {
			let repo_watcher =
				RepoWatcher::new(repo_work_dir(&repo)?.as_str());

			(never(), repo_watcher.receiver())
		}
		Updater::Ticker => (tick(TICK_INTERVAL), never()),
	};

	let spinner_ticker = tick(SPINNER_INTERVAL);
	let (dyn_jobs_thread, dyn_jobs_send, dyn_jobs_feedback) =
		async_jobs::AsyncJobList::new(tx_app.clone());

	let mut app = App::new(
		RefCell::new(repo),
		&tx_git,
		&tx_app,
		input.clone(),
		theme,
		key_config,
		dyn_jobs_send.clone(),
	)?;

	let mut spinner = Spinner::default();
	let mut first_update = true;

	log::trace!("app start: {} ms", app_start.elapsed().as_millis());

	let mut update_ticker : u64 = 0;

	loop {
		let event = if first_update {
			first_update = false;
			QueueEvent::Notify
		} else {
			select_event(
				&rx_input,
				&rx_git,
				&rx_app,
				&rx_ticker,
				&rx_watcher,
				&spinner_ticker,
			)?
		};

		{
			while let Ok(mut job_feedback) = dyn_jobs_feedback
				.recv_timeout(Duration::from_millis(0))
			{
				job_feedback.as_mut().visit(&mut app);
			}

			if matches!(event, QueueEvent::SpinnerUpdate) {
				spinner.update();
				spinner.draw(terminal)?;
				continue;
			}

			scope_time!("loop");


			match event {
				QueueEvent::InputEvent(ev) => {
					if matches!(
						ev,
						InputEvent::State(InputState::Polling)
					) {
						//Note: external ed closed, we need to re-hide cursor
						terminal.hide_cursor()?;
					}
					app.event(ev)?;
				}
				QueueEvent::Tick | QueueEvent::Notify => {
					update_ticker += TICK_INTERVAL_INT;
					if update_ticker > UPDATE_INTERVAL_INT {
						update_ticker %= UPDATE_INTERVAL_INT;
						app.update()?;
					}
					app.on_tick()?;
				}
				QueueEvent::AsyncEvent(ev) => {
					if !matches!(
						ev,
						AsyncNotification::Git(
							AsyncGitNotification::FinishUnchanged
						)
					) {
						app.update_async(ev)?;
					}
				}
				QueueEvent::SpinnerUpdate => unreachable!(),
			}

			draw(terminal, &app)?;

			spinner.set_state(app.any_work_pending());
			spinner.draw(terminal)?;

			if app.is_quit() {
				break;
			}
		}
	}
	dyn_jobs_send
		.send(Box::new(async_jobs::AsyncStopJob {})
			as async_jobs::BoxJob)?;
	if let Err(_) = dyn_jobs_thread.join() {}

	Ok(app.quit_state())
}

fn setup_terminal() -> Result<()> {
	enable_raw_mode()?;
	io::stdout().execute(EnterAlternateScreen)?;
	Ok(())
}

fn shutdown_terminal() {
	let leave_screen =
		io::stdout().execute(LeaveAlternateScreen).map(|_f| ());

	if let Err(e) = leave_screen {
		eprintln!("leave_screen failed:\n{e}");
	}

	let leave_raw_mode = disable_raw_mode();

	if let Err(e) = leave_raw_mode {
		eprintln!("leave_raw_mode failed:\n{e}");
	}
}

fn draw<B: Backend>(
	terminal: &mut Terminal<B>,
	app: &App,
) -> io::Result<()> {
	if app.requires_redraw() {
		terminal.resize(terminal.size()?)?;
	}

	terminal.draw(|f| {
		if let Err(e) = app.draw(f) {
			log::error!("failed to draw: {:?}", e);
		}
	})?;

	Ok(())
}

fn valid_path(repo_path: &RepoPath) -> bool {
	asyncgit::sync::is_repo(repo_path)
}

fn select_event(
	rx_input: &Receiver<InputEvent>,
	rx_git: &Receiver<AsyncGitNotification>,
	rx_app: &Receiver<AsyncAppNotification>,
	rx_ticker: &Receiver<Instant>,
	rx_notify: &Receiver<()>,
	rx_spinner: &Receiver<Instant>,
) -> Result<QueueEvent> {
	let mut sel = Select::new();

	sel.recv(rx_input);
	sel.recv(rx_git);
	sel.recv(rx_app);
	sel.recv(rx_ticker);
	sel.recv(rx_notify);
	sel.recv(rx_spinner);

	let oper = sel.select();
	let index = oper.index();

	let ev = match index {
		0 => oper.recv(rx_input).map(QueueEvent::InputEvent),
		1 => oper.recv(rx_git).map(|e| {
			QueueEvent::AsyncEvent(AsyncNotification::Git(e))
		}),
		2 => oper.recv(rx_app).map(|e| {
			QueueEvent::AsyncEvent(AsyncNotification::App(e))
		}),
		3 => oper.recv(rx_ticker).map(|_| QueueEvent::Notify),
		4 => oper.recv(rx_notify).map(|_| QueueEvent::Notify),
		5 => oper.recv(rx_spinner).map(|_| QueueEvent::SpinnerUpdate),
		_ => bail!("unknown select source"),
	}?;

	Ok(ev)
}

fn start_terminal<W: Write>(
	buf: W,
) -> io::Result<Terminal<CrosstermBackend<W>>> {
	let backend = CrosstermBackend::new(buf);
	let mut terminal = Terminal::new(backend)?;
	terminal.hide_cursor()?;
	terminal.clear()?;

	Ok(terminal)
}

// do log::error! and eprintln! in one line, pass sting, error and backtrace
macro_rules! log_eprintln {
	($string:expr, $e:expr, $bt:expr) => {
		log::error!($string, $e, $bt);
		eprintln!($string, $e, $bt);
	};
}

fn set_panic_handlers() -> Result<()> {
	// regular panic handler
	panic::set_hook(Box::new(|e| {
		let backtrace = Backtrace::new();
		log_eprintln!("panic: {:?}\ntrace:\n{:?}", e, backtrace);
		shutdown_terminal();
	}));

	// global threadpool
	rayon_core::ThreadPoolBuilder::new()
		.panic_handler(|e| {
			let backtrace = Backtrace::new();
			log_eprintln!("panic: {:?}\ntrace:\n{:?}", e, backtrace);
			shutdown_terminal();
			process::abort();
		})
		.num_threads(4)
		.build_global()?;

	Ok(())
}
