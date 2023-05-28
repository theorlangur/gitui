use crate::{
	error::Result,
	sync::{repo, CommitId, LogWalker, LogWalkerFilter, RepoPath},
	AsyncGitNotification,
};
use crossbeam_channel::Sender;
use scopetime::scope_time;
use std::{
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Mutex,
	},
	thread,
	time::Duration,
};

///
#[derive(PartialEq, Eq)]
pub enum FetchStatus {
	/// previous fetch still running
	Pending,
	/// no change expected
	NoChange,
	/// new walk was started
	Started,
}

///
pub struct AsyncLog {
	current: Arc<Mutex<Vec<CommitId>>>,
	current_head: Arc<Mutex<Option<CommitId>>>,
	sender: Sender<AsyncGitNotification>,
	pending: Arc<AtomicBool>,
	background: Arc<AtomicBool>,
	filter: Option<LogWalkerFilter>,
	repo: RepoPath,
	filter_updated: bool,
}

static LIMIT_COUNT: usize = 3000;
static SLEEP_FOREGROUND: Duration = Duration::from_millis(2);
static SLEEP_BACKGROUND: Duration = Duration::from_millis(1000);

impl AsyncLog {
	///
	pub fn new(
		repo: RepoPath,
		sender: &Sender<AsyncGitNotification>,
		filter: Option<LogWalkerFilter>,
	) -> Self {
		Self {
			repo,
			current: Arc::new(Mutex::new(Vec::new())),
			current_head: Arc::new(Mutex::new(None)),
			sender: sender.clone(),
			pending: Arc::new(AtomicBool::new(false)),
			background: Arc::new(AtomicBool::new(false)),
			filter,
			filter_updated: false,
		}
	}

	///
	pub fn count(&self) -> Result<usize> {
		Ok(self.current.lock()?.len())
	}

	///
	pub fn get_slice(
		&self,
		start_index: usize,
		amount: usize,
	) -> Result<Vec<CommitId>> {
		let list = self.current.lock()?;
		let list_len = list.len();
		let min = start_index.min(list_len);
		let max = min + amount;
		let max = max.min(list_len);
		Ok(list[min..max].to_vec())
	}

	///
	pub fn search_commit_forward<F>(
		&self,
		start_index: usize,
		predicate: F,
	) -> Option<usize>
	where
		F: Fn(&CommitId) -> bool,
	{
		if let Ok(list) = self.current.lock() {
			list.iter()
				.enumerate()
				.skip(start_index)
				.find(|i| predicate(i.1))
				.map(|i| i.0)
		} else {
			None
		}
	}

	///
	pub fn search_commit_backward<F>(
		&self,
		start_index: usize,
		predicate: F,
	) -> Option<usize>
	where
		F: Fn(&CommitId) -> bool,
	{
		if let Ok(list) = self.current.lock() {
			list.iter()
				.take(start_index)
				.enumerate()
				.rev()
				.find(|i| predicate(i.1))
				.map(|i| i.0)
		} else {
			None
		}
	}

	///
	pub fn position(&self, id: CommitId) -> Result<Option<usize>> {
		let list = self.current.lock()?;
		let position = list.iter().position(|&x| x == id);

		Ok(position)
	}

	///
	pub fn is_pending(&self) -> bool {
		self.pending.load(Ordering::Relaxed)
	}

	///
	pub fn set_background(&mut self) {
		self.background.store(true, Ordering::Relaxed);
	}

	///
	fn current_head(&self) -> Result<Option<CommitId>> {
		Ok(*self.current_head.lock()?)
	}

	///
	fn head_changed(&self) -> Result<bool> {
		if let Ok(head) = repo(&self.repo)?.head() {
			return Ok(
				head.target() != self.current_head()?.map(Into::into)
			);
		}
		Ok(false)
	}

	///
	pub fn update_filter(&mut self, filter: Option<LogWalkerFilter>) {
		self.filter = filter;
		self.filter_updated = true;
	}

	///
	pub fn fetch(&mut self) -> Result<FetchStatus> {
		self.background.store(false, Ordering::Relaxed);

		if self.is_pending() {
			return Ok(FetchStatus::Pending);
		}

		if !self.head_changed()? && !self.filter_updated {
			return Ok(FetchStatus::NoChange);
		}

		self.filter_updated = false;
		self.clear()?;

		let arc_current = Arc::clone(&self.current);
		let sender = self.sender.clone();
		let arc_pending = Arc::clone(&self.pending);
		let arc_background = Arc::clone(&self.background);
		let filter = self.filter.clone();
		let repo_path = self.repo.clone();

		self.pending.store(true, Ordering::Relaxed);

		if let Ok(head) = repo(&self.repo)?.head() {
			*self.current_head.lock()? =
				head.target().map(CommitId::new);
		}

		rayon_core::spawn(move || {
			scope_time!("async::revlog");

			Self::fetch_helper(
				&repo_path,
				&arc_current,
				&arc_background,
				&sender,
				filter,
			)
			.expect("failed to fetch");

			arc_pending.store(false, Ordering::Relaxed);

			Self::notify(&sender);
		});

		Ok(FetchStatus::Started)
	}

	fn fetch_helper(
		repo_path: &RepoPath,
		arc_current: &Arc<Mutex<Vec<CommitId>>>,
		arc_background: &Arc<AtomicBool>,
		sender: &Sender<AsyncGitNotification>,
		filter: Option<LogWalkerFilter>,
	) -> Result<()> {
		let mut entries = Vec::with_capacity(LIMIT_COUNT);
		let r = repo(repo_path)?;
		let mut walker =
			LogWalker::new(&r, LIMIT_COUNT)?.filter(filter);
		loop {
			entries.clear();
			let res_is_err = walker.read(&mut entries).is_err();

			if !res_is_err {
				let mut current = arc_current.lock()?;
				current.extend(entries.iter());
			}

			if res_is_err || entries.len() <= 1 {
				break;
			}
			Self::notify(sender);

			let sleep_duration =
				if arc_background.load(Ordering::Relaxed) {
					SLEEP_BACKGROUND
				} else {
					SLEEP_FOREGROUND
				};
			thread::sleep(sleep_duration);
		}

		Ok(())
	}

	fn clear(&mut self) -> Result<()> {
		self.current.lock()?.clear();
		*self.current_head.lock()? = None;
		Ok(())
	}

	fn notify(sender: &Sender<AsyncGitNotification>) {
		sender
			.send(AsyncGitNotification::Log)
			.expect("error sending");
	}
}
