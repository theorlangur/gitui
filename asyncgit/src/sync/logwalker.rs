use super::commit_files::get_commit_diff_by_path;
use super::CommitId;
use crate::sync::RepoPath;
use crate::{
	error::Error, error::Result, sync::commit_files::get_commit_diff,
};
use git2::{Commit, Oid, Repository};
use std::{
	cmp::Ordering,
	collections::{BinaryHeap, HashSet},
	sync::Arc,
};

struct TimeOrderedCommit<'a>(Commit<'a>);

impl<'a> Eq for TimeOrderedCommit<'a> {}

impl<'a> PartialEq for TimeOrderedCommit<'a> {
	fn eq(&self, other: &Self) -> bool {
		self.0.time().eq(&other.0.time())
	}
}

impl<'a> PartialOrd for TimeOrderedCommit<'a> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		self.0.time().partial_cmp(&other.0.time())
	}
}

impl<'a> Ord for TimeOrderedCommit<'a> {
	fn cmp(&self, other: &Self) -> Ordering {
		self.0.time().cmp(&other.0.time())
	}
}

///
pub type LogWalkerFilter = Arc<
	Box<
		dyn Fn(&Repository, &CommitId, &Commit) -> Result<bool>
			+ Send
			+ Sync,
	>,
>;

#[macro_export]
///
macro_rules! filter_compose_and {
	($e:expr) => {
		$e
	};
	($e:expr, $($y:expr),+) => {
		std::sync::Arc::new(Box::new(
			move |repo: &asyncgit::sync::Repository,
				  commit_id: &CommitId,
				  commit: &asyncgit::sync::Commit|
				  -> asyncgit::Result<bool> {
				if let Some(ref f) = $e {
					if f(repo, commit_id, commit)? != true {
						return Ok(false);
					}
				}
				$(
					if let Some(ref f) = $y {
						if f(repo, commit_id, commit)? != true {
							return Ok(false);
						}
					}
				)+
				Ok(true)
			},
		))
	};
}

///
pub fn filter_by_path(path: String, skip_merge: bool) -> LogWalkerFilter {
	Arc::new(Box::new(
		move |repo: &Repository,
		      _commit_id: &CommitId,
		      commit: &Commit|
		      -> Result<bool> {
            if skip_merge && commit.parent_count() > 1 {
                return Ok(false);
            }
			let diff = get_commit_diff_by_path(repo, commit, &path)?;
			let contains_file = diff.deltas().len() > 0;

			Ok(contains_file)
		},
	))
}

///
pub fn diff_contains_file(
	repo_path: RepoPath,
	file_path: String,
) -> LogWalkerFilter {
	Arc::new(Box::new(
		move |repo: &Repository,
		      commit_id: &CommitId,
		      _commit: &Commit|
		      -> Result<bool> {
			let diff = get_commit_diff(
				&repo_path,
				repo,
				*commit_id,
				Some(file_path.clone()),
				None,
			)?;

			let contains_file = diff.deltas().len() > 0;

			Ok(contains_file)
		},
	))
}

///
pub struct LogWalker<'a> {
	commits: BinaryHeap<TimeOrderedCommit<'a>>,
	visited: HashSet<Oid>,
	limit: usize,
	repo: &'a Repository,
	filter: Option<LogWalkerFilter>,
	stopper: Option<LogWalkerFilter>,
	filtered_limit: usize,
}

impl<'a> LogWalker<'a> {
	///
	pub fn new_with_start_by_path<Cb>(
		repo:  &RepoPath,
		start: Option<&CommitId>,
		config: Cb,
	) -> Result<Vec<CommitId>> 
		where
		Cb: FnOnce(LogWalker)->LogWalker
	{
		let r = crate::sync::repository::repo(repo)?;
		let c = if let Some(start) = start {
			r.find_commit(start.get_oid())?
		} else {
			r.head()?.peel_to_commit()?
		};
		let mut commits = BinaryHeap::with_capacity(10);
		commits.push(TimeOrderedCommit(c));
		let mut walker = LogWalker {
			commits,
			limit: 0,
			visited: HashSet::with_capacity(1000),
			repo: &r,
			filter: None,
			stopper: None,
			filtered_limit: 0
		};
		walker = config(walker);
		let mut res = Vec::new();
		walker.read_eof(&mut res).map(|_|res)
	}
	///
	pub fn new_with_start(
		repo: &'a Repository,
		start: Option<&CommitId>,
		limit: usize,
	) -> Result<Self> {
		let c = if let Some(start) = start {
			repo.find_commit(start.get_oid())?
		} else {
			repo.head()?.peel_to_commit()?
		};

		let mut commits = BinaryHeap::with_capacity(10);
		commits.push(TimeOrderedCommit(c));

		Ok(Self {
			commits,
			limit,
			visited: HashSet::with_capacity(1000),
			repo,
			filter: None,
			stopper: None,
			filtered_limit: 0
		})
	}

	///
	pub fn new(repo: &'a Repository, limit: usize) -> Result<Self> {
		Self::new_with_start(repo, None, limit)
	}

	///
	#[must_use]
	pub fn filter(self, filter: Option<LogWalkerFilter>) -> Self {
		Self { filter, ..self }
	}

	///
	#[must_use]
	pub fn filter_with_limit(self, filter: Option<LogWalkerFilter>, filtered_limit: usize) -> Self {
		Self { filter, filtered_limit, ..self }
	}

	///
	#[must_use]
	pub fn stopper(self, stopper: Option<LogWalkerFilter>) -> Self {
		Self { stopper, ..self }
	}

	///
	pub fn read_eof(
		&mut self,
		out: &mut Vec<CommitId>,
	) -> Result<usize> {
		if self.commits.peek().is_none() {
			Err(Error::Io(std::io::Error::from(
				std::io::ErrorKind::UnexpectedEof,
			)))
		} else {
			self.read(out)
		}
	}

	///
	pub fn read(&mut self, out: &mut Vec<CommitId>) -> Result<usize> {
		let mut count = 0_usize;
		let mut filtered_count = 0_usize;

		while let Some(c) = self.commits.pop() {
			for p in c.0.parents() {
				self.visit(p);
			}

			let id: CommitId = c.0.id().into();
			let commit_should_be_included =
				if let Some(ref filter) = self.filter {
					filter(self.repo, &id, &c.0)?
				} else {
					true
				};

			if commit_should_be_included {
				out.push(id);
				filtered_count += 1;
				if filtered_count == self.filtered_limit {
					break;
				}
			}

			count += 1;
			if count == self.limit {
				break;
			}

			if let Some(ref stopper) = self.stopper {
				if stopper(self.repo, &id, &c.0)? {
					break;
				}
			}
		}

		Ok(count)
	}

	//
	fn visit(&mut self, c: Commit<'a>) {
		if !self.visited.contains(&c.id()) {
			self.visited.insert(c.id());
			self.commits.push(TimeOrderedCommit(c));
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::error::Result;
	use crate::sync::RepoPath;
	use crate::sync::{
		commit, get_commits_info, stage_add_file,
		tests::repo_init_empty,
	};
	use pretty_assertions::assert_eq;
	use std::{fs::File, io::Write, path::Path};

	#[test]
	fn test_limit() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		commit(repo_path, "commit1").unwrap();
		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		let oid2 = commit(repo_path, "commit2").unwrap();

		let mut items = Vec::new();
		let mut walk = LogWalker::new(&repo, 1)?;
		walk.read(&mut items).unwrap();

		assert_eq!(items.len(), 1);
		assert_eq!(items[0], oid2);

		Ok(())
	}

	#[test]
	fn test_logwalker() -> Result<()> {
		let file_path = Path::new("foo");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		commit(repo_path, "commit1").unwrap();
		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(repo_path, file_path).unwrap();
		let oid2 = commit(repo_path, "commit2").unwrap();

		let mut items = Vec::new();
		let mut walk = LogWalker::new(&repo, 100)?;
		walk.read(&mut items).unwrap();

		let info = get_commits_info(repo_path, &items, 50).unwrap();
		dbg!(&info);

		assert_eq!(items.len(), 2);
		assert_eq!(items[0], oid2);

		let mut items = Vec::new();
		walk.read(&mut items).unwrap();

		assert_eq!(items.len(), 0);

		Ok(())
	}

	#[test]
	fn test_logwalker_with_filter() -> Result<()> {
		let file_path = Path::new("foo");
		let second_file_path = Path::new("baz");
		let (_td, repo) = repo_init_empty().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: RepoPath =
			root.as_os_str().to_str().unwrap().into();

		File::create(root.join(file_path))?.write_all(b"a")?;
		stage_add_file(&repo_path, file_path).unwrap();

		let _first_commit_id = commit(&repo_path, "commit1").unwrap();

		File::create(root.join(second_file_path))?.write_all(b"a")?;
		stage_add_file(&repo_path, second_file_path).unwrap();

		let second_commit_id = commit(&repo_path, "commit2").unwrap();

		File::create(root.join(file_path))?.write_all(b"b")?;
		stage_add_file(&repo_path, file_path).unwrap();

		let _third_commit_id = commit(&repo_path, "commit3").unwrap();

		let repo_path_clone = repo_path.clone();
		let diff_contains_baz =
			diff_contains_file(repo_path_clone, "baz".into());

		let mut items = Vec::new();
		let mut walker = LogWalker::new(&repo, 100)?
			.filter(Some(diff_contains_baz));
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 1);
		assert_eq!(items[0], second_commit_id);

		let mut items = Vec::new();
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 0);

		let diff_contains_bar =
			diff_contains_file(repo_path, "bar".into());

		let mut items = Vec::new();
		let mut walker = LogWalker::new(&repo, 100)?
			.filter(Some(diff_contains_bar));
		walker.read(&mut items).unwrap();

		assert_eq!(items.len(), 0);

		Ok(())
	}
}
