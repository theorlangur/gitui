//TODO: hopefully released in next rust (see https://github.com/rust-lang/rust-clippy/issues/9440)
#![allow(clippy::use_self)]

use crate::error::Result;
use git2::Repository;
use scopetime::scope_time;
use serde::{Deserialize, Serialize};

use super::{repository::repo, RepoPath};

///
#[derive(Clone, Serialize, Deserialize)]
pub struct GitExternCommands {
	///base push command (git push)
	pub push_base: Option<String>,
	///base fetch command (git fetch)
	pub fetch_base: Option<String>,
	///base checkout command (git checkout)
	pub checkout_base: Option<String>,
}

impl Default for GitExternCommands {
	fn default() -> Self {
		Self {
			push_base: None,
			fetch_base: None,
			checkout_base: None,
		}
	}
}

// see https://git-scm.com/docs/git-config#Documentation/git-config.txt-statusshowUntrackedFiles
/// represents the `status.showUntrackedFiles` git config state
#[derive(
	Hash, Copy, Clone, PartialEq, Eq, Serialize, Deserialize,
)]
pub enum ShowUntrackedFilesConfig {
	///
	No,
	///
	Normal,
	///
	All,
}

impl Default for ShowUntrackedFilesConfig {
	fn default() -> Self {
		Self::No
	}
}

impl ShowUntrackedFilesConfig {
	///
	pub const fn include_none(self) -> bool {
		matches!(self, Self::No)
	}

	///
	pub const fn include_untracked(self) -> bool {
		matches!(self, Self::Normal | Self::All)
	}

	///
	pub const fn recurse_untracked_dirs(self) -> bool {
		matches!(self, Self::All)
	}
}

pub fn untracked_files_config_repo(
	repo: &Repository,
) -> Result<ShowUntrackedFilesConfig> {
	let show_untracked_files =
		get_config_string_repo(repo, "status.showUntrackedFiles")?;

	if let Some(show_untracked_files) = show_untracked_files {
		if &show_untracked_files == "no" {
			return Ok(ShowUntrackedFilesConfig::No);
		} else if &show_untracked_files == "normal" {
			return Ok(ShowUntrackedFilesConfig::Normal);
		}
	}

	Ok(ShowUntrackedFilesConfig::All)
}

///
pub fn untracked_files_config(
	repo_path: &RepoPath,
) -> Result<ShowUntrackedFilesConfig> {
	let repo = repo(repo_path)?;
	untracked_files_config_repo(&repo)
}

/// get string from config
pub fn get_config_string(
	repo_path: &RepoPath,
	key: &str,
) -> Result<Option<String>> {
	let repo = repo(repo_path)?;
	get_config_string_repo(&repo, key)
}

pub fn get_config_string_repo(
	repo: &Repository,
	key: &str,
) -> Result<Option<String>> {
	scope_time!("get_config_string_repo");

	let cfg = repo.config()?;

	// this code doesnt match what the doc says regarding what
	// gets returned when but it actually works
	let entry_res = cfg.get_entry(key);

	let entry = match entry_res {
		Ok(ent) => ent,
		Err(_) => return Ok(None),
	};

	if entry.has_value() {
		Ok(entry.value().map(std::string::ToString::to_string))
	} else {
		Ok(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sync::tests::repo_init;

	#[test]
	fn test_get_config() {
		let bad_dir_cfg = get_config_string(
			&"oodly_noodly".into(),
			"this.doesnt.exist",
		);
		assert!(bad_dir_cfg.is_err());

		let (_td, repo) = repo_init().unwrap();
		let path = repo.path();
		let rpath = path.as_os_str().to_str().unwrap();
		let bad_cfg =
			get_config_string(&rpath.into(), "this.doesnt.exist");
		assert!(bad_cfg.is_ok());
		assert!(bad_cfg.unwrap().is_none());
		// repo init sets user.name
		let good_cfg = get_config_string(&rpath.into(), "user.name");
		assert!(good_cfg.is_ok());
		assert!(good_cfg.unwrap().is_some());
	}
}
