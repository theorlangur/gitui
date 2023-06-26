use super::{CommitId, RepoPath};
use crate::{
	error::{Error, Result},
	sync::repository::repo,
};
use git2::{Oid, Repository, Tree};
use scopetime::scope_time;
use std::{
	collections::HashSet,
	path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// `tree_files` returns a list of `FileTree`
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct TreeFile {
	/// path of this file
	pub path: PathBuf,
	/// unix filemode
	pub filemode: i32,
	// internal object id
	id: Oid,
}

/// fs-based file list (optionally with directories)
pub fn repo_files(
	repo_path: &RepoPath,
	with_directories: bool,
) -> Result<Vec<TreeFile>> {
	let res: Vec<_> = WalkDir::new(repo_path.gitpath())
		.into_iter()
		.filter_map(|e| e.ok())
		.filter(|e| {
			let _p = e.path().to_str();
			let _f = e.file_type();
			(with_directories || !e.file_type().is_dir())
				&& !e.path().iter().any(|i| {
					let s = i.to_str().unwrap_or("");
					s.len() > 1 && s.starts_with(".")
				})
		})
		.map(|e| TreeFile {
			path: e.path().to_path_buf(),
			filemode: 0,
			id: Oid::zero(),
		})
		.collect();
	//
	Ok(res)
}

/// guarantees sorting the result
pub fn tree_files(
	repo_path: &RepoPath,
	commit: CommitId,
	with_directories: bool,
) -> Result<Vec<TreeFile>> {
	scope_time!("tree_files");

	let repo = repo(repo_path)?;

	let commit = repo.find_commit(commit.into())?;
	let tree = commit.tree()?;

	let mut hfiles: HashSet<TreeFile> = HashSet::new();

	tree_recurse(&repo, &PathBuf::from("./"), &tree, &mut hfiles)?;

	if with_directories {
		let mut hdirs: HashSet<TreeFile> = HashSet::new();
		hfiles.iter().for_each(|i| {
			let mut p = i.path.clone();
			p.pop();
			hdirs.insert(TreeFile {
				path: p,
				filemode: 0,
				id: Oid::zero(),
			});
		});
		hfiles.extend(hdirs);
	}

	let mut files = hfiles.into_iter().collect::<Vec<_>>();
	sort_file_list(&mut files);

	Ok(files)
}

fn sort_file_list(files: &mut [TreeFile]) {
	files.sort_by(|a, b| a.path.cmp(&b.path));
}

/// will only work on utf8 content
pub fn tree_file_content(
	repo_path: &RepoPath,
	file: &TreeFile,
) -> Result<String> {
	scope_time!("tree_file_content");

	let repo = repo(repo_path)?;

	let blob = repo.find_blob(file.id)?;

	if blob.is_binary() {
		return Err(Error::BinaryFile);
	}

	let content = String::from_utf8_lossy(blob.content()).to_string();

	Ok(content)
}

///
fn tree_recurse(
	repo: &Repository,
	path: &Path,
	tree: &Tree,
	out: &mut HashSet<TreeFile>,
) -> Result<()> {
	for e in tree {
		let p = String::from_utf8_lossy(e.name_bytes());
		let path = path.join(p.to_string());
		match e.kind() {
			Some(git2::ObjectType::Blob) => {
				let id = e.id();
				let filemode = e.filemode();
				out.insert(TreeFile { path, filemode, id });
			}
			Some(git2::ObjectType::Tree) => {
				let obj = e.to_object(repo)?;
				let tree = obj.peel_to_tree()?;
				tree_recurse(repo, &path, &tree, out)?;
			}
			Some(_) | None => (),
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::{repo_init, write_commit_file};
	use pretty_assertions::{assert_eq, assert_ne};

	#[test]
	fn test_smoke() {
		let (_td, repo) = repo_init().unwrap();
		let root = repo.path().parent().unwrap();
		let repo_path: &RepoPath =
			&root.as_os_str().to_str().unwrap().into();

		let c1 =
			write_commit_file(&repo, "test.txt", "content", "c1");

		let files = tree_files(repo_path, c1, false).unwrap();

		assert_eq!(files.len(), 1);
		assert_eq!(files[0].path, PathBuf::from("./test.txt"));

		let c2 =
			write_commit_file(&repo, "test.txt", "content2", "c2");

		let content =
			tree_file_content(repo_path, &files[0]).unwrap();
		assert_eq!(&content, "content");

		let files_c2 = tree_files(repo_path, c2, false).unwrap();

		assert_eq!(files_c2.len(), 1);
		assert_ne!(files_c2[0], files[0]);
	}

	#[test]
	fn test_sorting() {
		let mut list = vec!["file", "folder/file", "folder/afile"]
			.iter()
			.map(|f| TreeFile {
				path: PathBuf::from(f),
				filemode: 0,
				id: Oid::zero(),
			})
			.collect::<Vec<_>>();

		sort_file_list(&mut list);

		assert_eq!(
			list.iter()
				.map(|f| f.path.to_string_lossy())
				.collect::<Vec<_>>(),
			vec![
				String::from("folder/afile"),
				String::from("folder/file"),
				String::from("file")
			]
		);
	}

	#[test]
	fn test_sorting_folders() {
		let mut list = vec!["bfolder/file", "afolder/file"]
			.iter()
			.map(|f| TreeFile {
				path: PathBuf::from(f),
				filemode: 0,
				id: Oid::zero(),
			})
			.collect::<Vec<_>>();

		sort_file_list(&mut list);

		assert_eq!(
			list.iter()
				.map(|f| f.path.to_string_lossy())
				.collect::<Vec<_>>(),
			vec![
				String::from("afolder/file"),
				String::from("bfolder/file"),
			]
		);
	}

	#[test]
	fn test_sorting_folders2() {
		let mut list = vec!["bfolder/sub/file", "afolder/file"]
			.iter()
			.map(|f| TreeFile {
				path: PathBuf::from(f),
				filemode: 0,
				id: Oid::zero(),
			})
			.collect::<Vec<_>>();

		sort_file_list(&mut list);

		assert_eq!(
			list.iter()
				.map(|f| f.path.to_string_lossy())
				.collect::<Vec<_>>(),
			vec![
				String::from("afolder/file"),
				String::from("bfolder/sub/file"),
			]
		);
	}
}
