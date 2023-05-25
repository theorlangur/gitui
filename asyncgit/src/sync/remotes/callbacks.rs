use super::push::ProgressNotification;
use crate::{error::Result, sync::cred::BasicAuthCredential};
use crossbeam_channel::Sender;
use git2::{Cred, Error as GitError, RemoteCallbacks};
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc, Mutex,
};

use ssh2_config::{ParseRule, SshConfig};
use std::{fs::File, io::BufReader};

///
#[derive(Default, Clone)]
pub struct CallbackStats {
	pub push_rejected_msg: Option<(String, String)>,
}

///
#[derive(Clone)]
pub struct Callbacks {
	sender: Option<Sender<ProgressNotification>>,
	basic_credential: Option<BasicAuthCredential>,
	stats: Arc<Mutex<CallbackStats>>,
	count_calls_to_credentials: Arc<AtomicUsize>,
}

impl Callbacks {
	///
	pub fn new(
		sender: Option<Sender<ProgressNotification>>,
		basic_credential: Option<BasicAuthCredential>,
	) -> Self {
		let stats = Arc::new(Mutex::new(CallbackStats::default()));

		Self {
			sender,
			basic_credential,
			stats,
			count_calls_to_credentials: Arc::new(AtomicUsize::new(0)),
		}
	}

	///
	pub fn get_stats(&self) -> Result<CallbackStats> {
		let stats = self.stats.lock()?;
		Ok(stats.clone())
	}

	///
	pub fn callbacks<'a>(&self) -> RemoteCallbacks<'a> {
		let mut callbacks = RemoteCallbacks::new();

		let this = self.clone();
		callbacks.push_transfer_progress(
			move |current, total, bytes| {
				this.push_transfer_progress(current, total, bytes);
			},
		);

		let this = self.clone();
		callbacks.update_tips(move |name, a, b| {
			this.update_tips(name, a, b);
			true
		});

		let this = self.clone();
		callbacks.transfer_progress(move |p| {
			this.transfer_progress(&p);
			true
		});

		let this = self.clone();
		callbacks.pack_progress(move |stage, current, total| {
			this.pack_progress(stage, total, current);
		});

		let this = self.clone();
		callbacks.push_update_reference(move |reference, msg| {
			this.push_update_reference(reference, msg);
			Ok(())
		});

		let this = self.clone();
		callbacks.credentials(
			move |url, username_from_url, allowed_types| {
				this.credentials(
					url,
					username_from_url,
					allowed_types,
				)
			},
		);

		callbacks.sideband_progress(move |data| {
			log::debug!(
				"sideband transfer: '{}'",
				String::from_utf8_lossy(data).trim()
			);
			true
		});

		callbacks
	}

	fn push_update_reference(
		&self,
		reference: &str,
		msg: Option<&str>,
	) {
		log::debug!(
			"push_update_reference: '{}' {:?}",
			reference,
			msg
		);

		if let Ok(mut stats) = self.stats.lock() {
			stats.push_rejected_msg = msg
				.map(|msg| (reference.to_string(), msg.to_string()));
		}
	}

	fn pack_progress(
		&self,
		stage: git2::PackBuilderStage,
		total: usize,
		current: usize,
	) {
		log::debug!("packing: {:?} - {}/{}", stage, current, total);
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::Packing {
				stage,
				total,
				current,
			})
		});
	}

	fn transfer_progress(&self, p: &git2::Progress) {
		log::debug!(
			"transfer: {}/{}",
			p.received_objects(),
			p.total_objects()
		);
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::Transfer {
				objects: p.received_objects(),
				total_objects: p.total_objects(),
			})
		});
	}

	fn update_tips(&self, name: &str, a: git2::Oid, b: git2::Oid) {
		log::debug!("update tips: '{}' [{}] [{}]", name, a, b);
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::UpdateTips {
				name: name.to_string(),
				a: a.into(),
				b: b.into(),
			})
		});
	}

	fn push_transfer_progress(
		&self,
		current: usize,
		total: usize,
		bytes: usize,
	) {
		log::debug!("progress: {}/{} ({} B)", current, total, bytes,);
		self.sender.clone().map(|sender| {
			sender.send(ProgressNotification::PushTransfer {
				current,
				total,
				bytes,
			})
		});
	}

	fn try_read_openssh_config(
		&self,
		url: &str,
		username_from_url: Option<&str>,
	) -> std::result::Result<Cred, GitError> {
		let config_path = if cfg!(target_os = "macos") {
			dirs_next::home_dir()
				.map(|h| h.join(".ssh").join("config"))
		} else {
			dirs_next::home_dir()
				.map(|h| h.join(".ssh").join("config"))
		};

		if config_path.is_none() {
			return Cred::default();
		}

		let config_path = config_path.unwrap();

		let mut reader = BufReader::new(
			File::open(config_path)
				.expect("Could not open configuration file"), //we should manually unwrap
		);

		let config = SshConfig::default()
			.parse(&mut reader, ParseRule::STRICT)
			.expect("Failed to parse configuration");
		let disected_url = git_url_parse::GitUrl::parse(url);
		if disected_url.is_err() {
			return Err(GitError::from_str(&format!(
				"Wrong url: {:?}",
				disected_url.err().unwrap()
			)));
		}

		let disected_url = disected_url.unwrap();
		let host_str = disected_url.host;
		if host_str.is_none() {
			return Err(GitError::from_str(&format!(
				"No host found in url: {:?}",
				url
			)));
		}

		//let default_params = config.default_params();
		// Query parameters for your host
		// If there's no rule for your host, default params are returned
		let params = config.query(host_str.unwrap());

		if username_from_url.is_some()
			&& params.identity_file.is_some()
		{
			Cred::ssh_key(
				username_from_url.unwrap(),
				None,
				params.identity_file.unwrap()[0].as_path(),
				None,
			)
		} else {
			Err(GitError::from_str("Couldn't find credentials"))
		}
	}

	// If credentials are bad, we don't ask the user to re-fill their creds. We push an error and they will be able to restart their action (for example a push) and retype their creds.
	// This behavior is explained in a issue on git2-rs project : https://github.com/rust-lang/git2-rs/issues/347
	// An implementation reference is done in cargo : https://github.com/rust-lang/cargo/blob/9fb208dddb12a3081230a5fd8f470e01df8faa25/src/cargo/sources/git/utils.rs#L588
	// There is also a guide about libgit2 authentication : https://libgit2.org/docs/guides/authentication/
	fn credentials(
		&self,
		url: &str,
		username_from_url: Option<&str>,
		allowed_types: git2::CredentialType,
	) -> std::result::Result<Cred, GitError> {
		log::debug!(
			"creds: '{}' {:?} ({:?})",
			url,
			username_from_url,
			allowed_types
		);

		// This boolean is used to avoid multiple calls to credentials callback.
		let prev_call_count = self
			.count_calls_to_credentials
			.fetch_add(1, Ordering::Relaxed);
		if prev_call_count >= 2 {
			return Err(GitError::from_str("Bad credentials."));
		}

		match &self.basic_credential {
			_ if prev_call_count == 0
				&& allowed_types.is_ssh_key() =>
			{
				username_from_url.map_or_else(
					|| {
						Err(GitError::from_str(
							" Couldn't extract username from url.",
						))
					},
					Cred::ssh_key_from_agent,
				)
			}
			_ if prev_call_count == 1
				&& allowed_types.is_ssh_key() =>
			{
				//first attempt didn't pan out
				//maybe OpenSSH config will help us?
				self.try_read_openssh_config(url, username_from_url)
			}
			Some(BasicAuthCredential {
				username: Some(user),
				password: Some(pwd),
			}) if allowed_types.is_user_pass_plaintext() => {
				Cred::userpass_plaintext(user, pwd)
			}
			Some(BasicAuthCredential {
				username: Some(user),
				password: _,
			}) if allowed_types.is_username() => Cred::username(user),
			_ if allowed_types.is_default() => Cred::default(),
			_ => Err(GitError::from_str("Couldn't find credentials")),
		}
	}
}
