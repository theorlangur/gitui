use super::filter_options::FilterOptionsPopupComponent;
use super::search_options::SearchOptionsPopupComponent;
use super::utils::logitems::{ItemBatch, LogEntry};
use super::TextInputComponent;
use crate::queue::{
	create_local_queue, CustomConfirmData, InternalEvent, LocalEvent,
	NeedsUpdate, SharedLocalQueue,
};
use crate::{
	components::{
		utils::string_width_align, CommandBlocking, CommandInfo,
		Component, DrawableComponent, EventState, ScrollType,
	},
	keys::{key_match, SharedKeyConfig},
	queue::Queue,
	strings::{self, symbol},
	try_or_popup,
	ui::style::{SharedTheme, Theme},
	ui::{calc_scroll_top, draw_scrollbar, Orientation},
};
use anyhow::Result;
use asyncgit::sync::branch::checkout_branch_cmd;
use asyncgit::sync::{
	self, checkout_commit, cherrypick, filter_by_path, get_commit_info, get_head, BranchDetails, BranchInfo, CommitId, LogWalkerFilter, RepoPathRef, RepoState, Tags
};

use chrono::{DateTime, Local};
use crossterm::event::{Event, KeyEvent};
use itertools::Itertools;
use ratatui::layout::{Constraint, Layout};
use ratatui::{
	backend::Backend,
	layout::{Alignment, Rect},
	text::{Span, Spans},
	widgets::{Block, Borders, Paragraph},
	Frame,
};
use std::path::PathBuf;
use std::{
	borrow::Cow, cell::Cell, cmp, collections::BTreeMap,
	convert::TryFrom, time::Instant,
};

const ELEMENTS_PER_LINE: usize = 9;

#[derive(PartialEq)]
enum Focused {
	InputSearch,
	InputFilter,
	List,
}

#[derive(PartialEq)]
enum KeyComboState {
	Empty,
	SearchInitForward,
	//SearchInitBackward,
	FilterInit,
}

/*
enum RebaseAction {
	Drop,
	Squash,
	Fixup,
	Reword
}*/

#[derive(PartialEq, Clone)]
pub enum ExternalSearchRequest {
	Empty,
	Forward,
	Backward,
}

///
pub struct CommitList {
	repo: RepoPathRef,
	title: Box<str>,
	selection: usize,
	count_total: usize,
	items: ItemBatch,
	marked: Vec<(usize, CommitId)>,
	//rebase_marked: Vec<(usize, CommitId, RebaseAction)>,
	scroll_state: (Instant, f32),
	tags: Option<Tags>,
	local_branches: BTreeMap<CommitId, Vec<BranchInfo>>,
	remote_branches: BTreeMap<CommitId, Vec<BranchInfo>>,
	current_size: Cell<Option<(u16, u16)>>,
	scroll_top: Cell<usize>,
	theme: SharedTheme,
	queue: Queue,
	key_config: SharedKeyConfig,
	search_field: TextInputComponent,
	filter_field: TextInputComponent,
	search_options: SearchOptionsPopupComponent,
	filter_options: FilterOptionsPopupComponent,
	focused_field: Focused,
	current_search: String,
	filter_updated: bool,
	combo_state: KeyComboState,
	extended_search_request: ExternalSearchRequest,
	last_selected_commit: Option<CommitId>,
	external_focus: bool,
	local_queue: SharedLocalQueue,
	path_filter: PathBuf,
	branches_update_needed: bool,
}

impl CommitList {
	///
	pub fn new(
		repo: RepoPathRef,
		title: &str,
		theme: SharedTheme,
		queue: Queue,
		key_config: SharedKeyConfig,
	) -> Self {
		Self {
			repo,
			items: ItemBatch::default(),
			marked: Vec::with_capacity(2),
			//rebase_marked: Vec::with_capacity(2),
			selection: 0,
			count_total: 0,
			scroll_state: (Instant::now(), 0_f32),
			tags: None,
			local_branches: BTreeMap::default(),
			remote_branches: BTreeMap::default(),
			current_size: Cell::new(None),
			scroll_top: Cell::new(0),
			theme: theme.clone(),
			queue,
			key_config: key_config.clone(),
			title: title.into(),
			search_field: TextInputComponent::new(
				theme.clone(),
				key_config.clone(),
				"Search in commits...",
				"Enter text to search here",
				false,
			)
			.with_input_type(super::InputType::Singleline)
			.make_embed(),
			filter_field: TextInputComponent::new(
				theme.clone(),
				key_config.clone(),
				"Filter commits...",
				"Enter text to filter by",
				false,
			)
			.with_input_type(super::InputType::Singleline)
			.make_embed(),
			search_options: SearchOptionsPopupComponent::new(
				theme.clone(),
				key_config.clone(),
			),
			filter_options: FilterOptionsPopupComponent::new(
				theme.clone(),
				key_config.clone(),
			),
			focused_field: Focused::List,
			current_search: String::new(),
			filter_updated: true,
			combo_state: KeyComboState::Empty,
			extended_search_request: ExternalSearchRequest::Empty,
			last_selected_commit: None,
			external_focus: true,
			local_queue: create_local_queue(),
			path_filter: PathBuf::new(),
			branches_update_needed: false,
		}
	}

	fn update_path_filter(&mut self, p: PathBuf) {
		if self.path_filter != p {
			self.path_filter = p;
			self.filter_updated = true;
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		}
	}

	///
	pub fn update(&mut self) -> Result<()> {
		self.process_local_queue();
		Ok(())
	}

	pub fn needs_branch_update(&mut self) -> bool {
		let v = self.branches_update_needed;
		self.branches_update_needed = false;
		v
	}

	fn process_local_queue(&mut self) {
		loop {
			//suboptimal...
			let mut q = self.local_queue.borrow_mut();
			let e = q.pop_front();
			drop(q);
			if let Some(e) = e {
				match e {
					LocalEvent::Confirmed(ref s)
						if s == "cherrypick" =>
					{
						self.branches_update_needed = true;
						self.cherrypick_marked()
					}
					LocalEvent::Confirmed(ref s) if s == "drop" => {
						self.branches_update_needed = true;
						self.drop_marked()
					}
					LocalEvent::Confirmed(ref s) if s == "fixup" => {
						self.branches_update_needed = true;
						self.fixup_marked()
					}
					LocalEvent::PickFile(p) => {
						self.update_path_filter(p);
					}
					_ => {
						panic!("Unexpected local event");
					}
				}
			} else {
				break;
			}
		}
	}

	///
	pub fn show_search(&mut self) {
		if let Ok(_) = self.search_field.show() {
			self.search_field.clear();
			self.focused_field = Focused::InputSearch;
		}
	}

	pub fn stop_search(&mut self) {
		self.current_search.clear();
		self.search_field.hide();
		self.focused_field = Focused::List;
	}

	pub fn show_filter(&mut self) {
		if let Ok(_) = self.filter_field.show() {
			self.filter_field.clear();
			self.focused_field = Focused::InputFilter;
		}
	}

	pub fn stop_filter(&mut self) {
		self.filter_field.clear();
		self.focused_field = Focused::List;
		if self.filter_field.is_visible() {
			self.filter_field.hide();
			self.filter_updated = true;
		}
	}

	pub fn toggle_input_focus(&mut self) {
		self.focused_field = match self.focused_field {
			Focused::InputFilter
				if self.search_field.is_visible() =>
			{
				Focused::InputSearch
			}
			Focused::InputFilter => Focused::List,
			Focused::List if self.filter_field.is_visible() => {
				Focused::InputFilter
			}
			Focused::List if self.search_field.is_visible() => {
				Focused::InputSearch
			}
			_ => Focused::List,
		};
	}

	///
	pub fn items(&mut self) -> &mut ItemBatch {
		&mut self.items
	}

	pub fn get_path_filter(&self) -> Option<LogWalkerFilter> {
		if self.path_filter.as_os_str().is_empty() {
			None
		} else {
			let p: String = self
				.path_filter
				.iter()
				.skip_while(|i| i.to_str().unwrap_or("") == ".")
				.map(|i| i.to_str().unwrap_or(""))
				.collect::<Vec<_>>()
				.join(std::path::MAIN_SEPARATOR_STR);
			Some(filter_by_path(p, true))
		}
	}

	pub fn get_filter(&self) -> Option<LogWalkerFilter> {
		if self.filter_field.is_visible()
			&& !self.filter_field.get_text().is_empty()
		{
			let filter_txt =
				self.filter_field.get_text().to_lowercase();
			let filter_author = self.filter_options.author;
			let filter_msg = self.filter_options.message;
			Some(std::sync::Arc::new(Box::new(
				move |_repo,
				      _commit_id: &CommitId,
				      commit: &asyncgit::sync::Commit|
				      -> Result<bool, asyncgit::Error> {
					if filter_author
						&& commit
							.author()
							.name()
							.unwrap()
							.to_lowercase()
							.contains(&filter_txt)
					{
						Ok(true)
					} else if filter_msg
						&& commit
							.message()
							.unwrap()
							.to_lowercase()
							.contains(&filter_txt)
					{
						Ok(true)
					} else {
						Ok(false)
					}
				},
			)))
		} else {
			None
		}
	}

	fn cherrypick_marked(&mut self) {
		//implement
		let repo = self.repo.borrow();
		//save current head
		for i in self.marked.iter().rev() {
			if let Err(err) = cherrypick(&repo, i.1.clone(), true) {
				//rollback
				//show error
				self.queue.push(
					crate::queue::InternalEvent::ShowErrorMsg(
						format!(
							"{}\n{}",
							"Could not perform cherrypick(s)", err
						),
					),
				);
				self.queue
					.push(InternalEvent::Update(NeedsUpdate::ALL));
				return;
			}
		}
		self.marked.clear();
		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
	}

	fn fixup_marked(&mut self) {
		let oldest_commit = self
			.marked
			.iter()
			.max_by(|x, y| x.0.cmp(&y.0))
			.unwrap()
			.1
			.clone();
		let oldest_to_use: CommitId = asyncgit::sync::parent_ids(
			&self.repo.borrow(),
			oldest_commit,
		)
		.unwrap()[0];
		let base: CommitId = asyncgit::sync::parent_ids(
			&self.repo.borrow(),
			oldest_to_use,
		)
		.unwrap()[0];
		let list: Vec<_> = self.marked.iter().map(|i| &i.1).collect();
		if let Err(e) =
			asyncgit::sync::extern_git::rebase_fixup_commits(
				self.repo.borrow().gitpath().to_str().unwrap(),
				list,
				&base,
			) {
			self.queue.push(InternalEvent::ShowErrorMsg(format!(
				"Dropping commits failed: {}",
				e
			)));
		} else {
			self.marked.clear();
		}
		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
	}

	fn drop_marked(&mut self) {
		let oldest_commit = self
			.marked
			.iter()
			.max_by(|x, y| x.0.cmp(&y.0))
			.unwrap()
			.1
			.clone();
		let base: CommitId = asyncgit::sync::parent_ids(
			&self.repo.borrow(),
			oldest_commit,
		)
		.unwrap()[0];
		let list: Vec<_> = self.marked.iter().map(|i| &i.1).collect();
		if let Err(e) =
			asyncgit::sync::extern_git::rebase_drop_commits(
				self.repo.borrow().gitpath().to_str().unwrap(),
				list,
				&base,
			) {
			self.queue.push(InternalEvent::ShowErrorMsg(format!(
				"Dropping commits failed: {}",
				e
			)));
		} else {
			self.marked.clear();
		}
		self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
	}

	///
	pub fn set_title(&mut self, t: Box<str>) {
		self.title = t;
	}

	///
	pub const fn selection(&self) -> usize {
		self.selection
	}

	/// will return view size or None before the first render
	pub fn current_size(&self) -> Option<(u16, u16)> {
		self.current_size.get()
	}

	///
	pub fn set_count_total(&mut self, total: usize) {
		self.count_total = total;
		self.selection =
			cmp::min(self.selection, self.selection_max());
	}

	///
	#[allow(clippy::missing_const_for_fn)]
	pub fn selection_max(&self) -> usize {
		self.count_total.saturating_sub(1)
	}

	///
	pub const fn tags(&self) -> Option<&Tags> {
		self.tags.as_ref()
	}

	///
	pub fn clear(&mut self) {
		self.items.clear();
	}

	///
	pub fn set_tags(&mut self, tags: Tags) {
		self.tags = Some(tags);
	}

	///
	pub fn selected_entry(&self) -> Option<&LogEntry> {
		self.items.iter().nth(
			self.selection.saturating_sub(self.items.index_offset()),
		)
	}

	pub fn clear_last_selected_commit(&mut self) {
		self.last_selected_commit = None;
	}

	///
	pub fn get_last_selected_commit(&self) -> Option<&CommitId> {
		if !self.is_filter_active() && !self.is_search_active() {
			self.last_selected_commit.as_ref()
		} else {
			None
		}
	}

	///
	pub fn selected_entry_marked(&self) -> bool {
		self.selected_entry()
			.and_then(|e| self.is_marked(&e.id))
			.unwrap_or_default()
	}

	///
	pub fn marked_count(&self) -> usize {
		self.marked.len()
	}

	///
	pub fn marked(&self) -> &[(usize, CommitId)] {
		&self.marked
	}

	///
	pub fn clear_marked(&mut self) {
		self.marked.clear();
	}

	///
	pub fn marked_commits(&self) -> Vec<CommitId> {
		let (_, commits): (Vec<_>, Vec<CommitId>) =
			self.marked.iter().copied().unzip();

		commits
	}

	fn move_selection(&mut self, scroll: ScrollType) -> Result<bool> {
		self.update_scroll_speed();

		#[allow(clippy::cast_possible_truncation)]
		let speed_int = usize::try_from(self.scroll_state.1 as i64)?.max(1);

		let page_offset = usize::from(
			self.current_size.get().unwrap_or_default().1,
		)
		.saturating_sub(1);

		let new_selection = match scroll {
			ScrollType::Up => {
				self.selection.saturating_sub(speed_int)
			}
			ScrollType::Down => {
				self.selection.saturating_add(speed_int)
			}
			ScrollType::PageUp => {
				self.selection.saturating_sub(page_offset)
			}
			ScrollType::PageDown => {
				self.selection.saturating_add(page_offset)
			}
			ScrollType::Home => 0,
			ScrollType::End => self.selection_max(),
		};

		let new_selection =
			cmp::min(new_selection, self.selection_max());

		let needs_update = new_selection != self.selection;
		self.select_entry(new_selection);

		Ok(needs_update)
	}

	fn mark(&mut self) {
		if let Some(e) = self.selected_entry() {
			let id = e.id;
			let selected = self
				.selection
				.saturating_sub(self.items.index_offset());
			if self.is_marked(&id).unwrap_or_default() {
				self.marked.retain(|marked| marked.1 != id);
			} else {
				self.marked.push((selected, id));

				self.marked.sort_unstable_by(|first, second| {
					first.0.cmp(&second.0)
				});
			}
		}
	}

	fn update_scroll_speed(&mut self) {
		const REPEATED_SCROLL_THRESHOLD_MILLIS: u128 = 300;
		const SCROLL_SPEED_START: f32 = 0.1_f32;
		const SCROLL_SPEED_MAX: f32 = 10_f32;
		const SCROLL_SPEED_MULTIPLIER: f32 = 1.05_f32;

		let now = Instant::now();

		let since_last_scroll =
			now.duration_since(self.scroll_state.0);

		self.scroll_state.0 = now;

		let speed = if since_last_scroll.as_millis()
			< REPEATED_SCROLL_THRESHOLD_MILLIS
		{
			self.scroll_state.1 * SCROLL_SPEED_MULTIPLIER
		} else {
			SCROLL_SPEED_START
		};

		self.scroll_state.1 = speed.min(SCROLL_SPEED_MAX);
	}

	fn is_marked(&self, id: &CommitId) -> Option<bool> {
		if self.marked.is_empty() {
			None
		} else {
			let found =
				self.marked.iter().any(|entry| entry.1 == *id);
			Some(found)
		}
	}

	#[allow(clippy::too_many_arguments)]
	fn get_entry_to_add<'a>(
		e: &'a LogEntry,
		selected: bool,
		external_focus: bool,
		tags: Option<String>,
		local_branches: Option<String>,
		remote_branches: Option<String>,
		theme: &Theme,
		width: usize,
		now: DateTime<Local>,
		marked: Option<bool>,
	) -> Spans<'a> {
		let mut txt: Vec<Span> = Vec::with_capacity(
			ELEMENTS_PER_LINE + if marked.is_some() { 2 } else { 0 },
		);

		let splitter_txt = Cow::from(symbol::EMPTY_SPACE);
		let splitter = Span::styled(
			splitter_txt,
			theme.text(external_focus, selected),
		);

		// marker
		if let Some(marked) = marked {
			txt.push(Span::styled(
				Cow::from(if marked {
					symbol::CHECKMARK
				} else {
					symbol::EMPTY_SPACE
				}),
				theme.log_marker(selected),
			));
			txt.push(splitter.clone());
		}

		// commit hash
		txt.push(Span::styled(
			Cow::from(&*e.hash_short),
			theme.commit_hash(selected),
		));

		txt.push(splitter.clone());

		// commit timestamp
		txt.push(Span::styled(
			Cow::from(e.time_to_string(now)),
			theme.commit_time(selected),
		));

		txt.push(splitter.clone());

		let author_width =
			(width.saturating_sub(19) / 3).clamp(3, 20);
		let author = string_width_align(&e.author, author_width);

		// commit author
		txt.push(Span::styled::<String>(
			author,
			theme.commit_author(selected),
		));

		txt.push(splitter.clone());

		// commit tags
		if let Some(tags) = tags {
			txt.push(splitter.clone());
			txt.push(Span::styled(tags, theme.tags(selected)));
		}

		if let Some(local_branches) = local_branches {
			txt.push(splitter.clone());
			txt.push(Span::styled(
				local_branches,
				theme.branch(selected, true),
			));
		}

		if let Some(remote_branches) = remote_branches {
			txt.push(splitter.clone());
			txt.push(Span::styled(
				remote_branches,
				theme.branch(selected, true),
			));
		}

		txt.push(splitter);

		let message_width = width.saturating_sub(
			txt.iter().map(|span| span.content.len()).sum(),
		);

		// commit msg
		txt.push(Span::styled(
			format!("{:message_width$}", &e.msg),
			theme.text(true, selected),
		));

		Spans::from(txt)
	}

	fn get_text(&self, height: usize, width: usize) -> Vec<Spans> {
		let selection = self.relative_selection();

		let mut txt: Vec<Spans> = Vec::with_capacity(height);

		let now = Local::now();

		let any_marked = !self.marked.is_empty();

		for (idx, e) in self
			.items
			.iter()
			.skip(self.scroll_top.get())
			.take(height)
			.enumerate()
		{
			let tags =
				self.tags.as_ref().and_then(|t| t.get(&e.id)).map(
					|tags| {
						tags.iter()
							.map(|t| format!("<{}>", t.name))
							.join(" ")
					},
				);

			let local_branches =
				self.local_branches.get(&e.id).map(|local_branch| {
					local_branch
						.iter()
						.map(|local_branch| {
							format!("{{{0}}}", local_branch.name)
						})
						.join(" ")
				});

			let remote_branches = self
				.remote_branches
				.get(&e.id)
				.and_then(|remote_branches| {
					let filtered_branches: Vec<_> = remote_branches
						.iter()
						.filter(|remote_branch| {
							self.local_branches
								.get(&e.id)
								.map_or(true, |local_branch| {
									local_branch.iter().any(
										|local_branch| {
											let has_corresponding_local_branch = match &local_branch.details {
												BranchDetails::Local(details) =>
													details
														.upstream
														.as_ref()
														.map_or(false, |upstream| upstream.reference == remote_branch.reference),
												BranchDetails::Remote(_) =>
														false,
											};

											!has_corresponding_local_branch
										},
									)
								})
						})
						.map(|remote_branch| {
							format!("[{0}]", remote_branch.name)
						})
						.collect();

					if filtered_branches.is_empty() {
						None
					} else {
						Some(filtered_branches.join(" "))
					}
				});

			let marked = if any_marked {
				self.is_marked(&e.id)
			} else {
				None
			};

			txt.push(Self::get_entry_to_add(
				e,
				idx + self.scroll_top.get() == selection,
				self.external_focus,
				tags,
				local_branches,
				remote_branches,
				&self.theme,
				width,
				now,
				marked,
			));
		}

		txt
	}

	#[allow(clippy::missing_const_for_fn)]
	fn relative_selection(&self) -> usize {
		self.selection.saturating_sub(self.items.index_offset())
	}

	pub fn is_filter_active(&self) -> bool {
		self.filter_field.is_visible()
	}

	pub fn is_search_active(&self) -> bool {
		self.search_field.is_visible()
	}

	pub fn select_entry(&mut self, position: usize) {
		self.selection = position;
		if let Some(e) = self.selected_entry() {
			self.last_selected_commit = Some(e.id.clone());
		} else {
			self.last_selected_commit = None;
		}
	}

	pub fn checkout(&mut self) {
		if let Some(commit_hash) =
			self.selected_entry().map(|entry| entry.id)
		{
			let cmd = String::from("git checkout");
			if cmd.is_empty() {
				try_or_popup!(
					self,
					"failed to checkout commit:",
					checkout_commit(&self.repo.borrow(), commit_hash)
				);
			} else {
				try_or_popup!(
					self,
					"failed to checkout commit:",
					checkout_branch_cmd(
						cmd,
						commit_hash.to_string().as_str()
					)
				);
			}
		}
	}

	pub fn has_extended_search_request(
		&mut self,
	) -> ExternalSearchRequest {
		let res = self.extended_search_request.clone();
		self.extended_search_request = ExternalSearchRequest::Empty;
		res
	}

	pub fn get_search_needle(&self) -> String {
		self.current_search.to_lowercase()
	}

	pub fn is_search_hash_only(&self) -> bool {
		return !self.search_options.message
			&& !self.search_options.author
			&& self.search_options.sha;
	}

	pub fn search_commit_check(
		&self,
		needle: &str,
		author: &str,
		message: &str,
		hash: &str,
	) -> bool {
		(self.search_options.message
			&& message.to_lowercase().contains(needle))
			|| (self.search_options.author
				&& author.to_lowercase().contains(needle))
			|| (self.search_options.sha && hash.contains(&needle))
	}

	pub fn search_commit_forward(&mut self) {
		if self.current_search.is_empty() {
			return ();
		}
		let local_selection =
			self.selection - self.items.index_offset();
		let needle = self.get_search_needle();
		let res = self
			.items
			.iter()
			.enumerate()
			.skip(local_selection + 1)
			.filter(|item| {
				self.search_commit_check(
					&needle,
					&item.1.author,
					&item.1.msg,
					&item.1.hash_full,
				)
			})
			.map(|item| item.0)
			.nth(0);
		if let Some(idx) = res {
			self.select_entry(self.items.index_offset() + idx);
		} else {
			self.extended_search_request =
				ExternalSearchRequest::Forward;
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		}
	}

	pub fn search_commit_backward(&mut self) {
		if self.current_search.is_empty() {
			return ();
		}
		let local_selection =
			self.selection - self.items.index_offset();
		let needle = self.current_search.to_lowercase();
		let res = self
			.items
			.iter()
			.take(local_selection)
			.enumerate()
			.rev()
			.filter(|item| {
				self.search_commit_check(
					&needle,
					&item.1.author,
					&item.1.msg,
					&item.1.hash_full,
				)
			})
			.map(|item| item.0)
			.nth(0);
		if let Some(idx) = res {
			self.select_entry(self.items.index_offset() + idx);
		} else if self.items.index_offset() > 0 {
			self.extended_search_request =
				ExternalSearchRequest::Backward;
			self.queue.push(InternalEvent::Update(NeedsUpdate::ALL));
		}
	}

	pub fn set_local_branches(
		&mut self,
		local_branches: Vec<BranchInfo>,
	) {
		self.local_branches.clear();

		for local_branch in local_branches {
			self.local_branches
				.entry(local_branch.top_commit)
				.or_default()
				.push(local_branch);
		}
	}

	pub fn set_remote_branches(
		&mut self,
		remote_branches: Vec<BranchInfo>,
	) {
		self.remote_branches.clear();

		for remote_branch in remote_branches {
			self.remote_branches
				.entry(remote_branch.top_commit)
				.or_default()
				.push(remote_branch);
		}
	}

	pub fn filter_was_updated(&mut self) -> bool {
		let v = self.filter_updated;
		self.filter_updated = false;
		v
	}

	fn get_commit_short_summary(
		&self,
		c: &CommitId,
	) -> Result<String> {
		let info = get_commit_info(&self.repo.borrow(), c)?;
		Ok(info.get_summary())
	}

	fn get_marked_summary(&self) -> String {
		const SUMMARY_COMMIT_COUNT: usize = 4;
		let mut commit_summary = self
			.marked
			.iter()
			.take(SUMMARY_COMMIT_COUNT)
			.map(|i| {
				self.get_commit_short_summary(&i.1)
					.ok()
					.unwrap_or_default()
			})
			.join("\n");
		let rest_commits = self.marked.len()
			- self.marked.len().min(SUMMARY_COMMIT_COUNT);
		if rest_commits > 0 {
			commit_summary +=
				&format!("\nand {} more commits", rest_commits);
		}
		commit_summary
	}

	fn try_handle_combo_event(
		&mut self,
		k: &KeyEvent,
	) -> Option<EventState> {
		match self.combo_state {
			KeyComboState::SearchInitForward => {
				self.combo_state = KeyComboState::Empty;
				if key_match(
					k,
					self.key_config.keys.start_search_forward_init,
				) {
					self.search_options.enable_all();
					self.show_search();
					Some(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.search_filter_author,
				) {
					self.search_options.author_only();
					self.show_search();
					Some(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.search_filter_msg,
				) {
					self.search_options.message_only();
					self.show_search();
					Some(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.search_sha,
				) {
					self.search_options.sha_only();
					self.show_search();
					Some(EventState::Consumed)
				} else {
					None
				}
			}
			KeyComboState::FilterInit => {
				self.combo_state = KeyComboState::Empty;
				if key_match(
					k,
					self.key_config.keys.filter_commits_init,
				) {
					self.filter_options.enable_all();
					self.show_filter();
					Some(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.search_filter_author,
				) {
					self.filter_options.author_only();
					self.show_filter();
					Some(EventState::Consumed)
				} else if key_match(
					k,
					self.key_config.keys.search_filter_msg,
				) {
					self.filter_options.message_only();
					self.show_filter();
					Some(EventState::Consumed)
				} else {
					None
				}
			}
			KeyComboState::Empty => None,
		}
	}

	fn list_event(&mut self, ev: &Event) -> Result<EventState> {
		if let Event::Key(k) = ev {
			if let Some(r) = self.try_handle_combo_event(k) {
				return Ok(r);
			}
			let git_state = sync::repo_state(&self.repo.borrow())
				.unwrap_or(RepoState::Clean);
			let selection_changed =
				if key_match(k, self.key_config.keys.move_up) {
					self.move_selection(ScrollType::Up)?
				} else if key_match(k, self.key_config.keys.move_down)
				{
					self.move_selection(ScrollType::Down)?
				} else if key_match(k, self.key_config.keys.shift_up)
					|| key_match(k, self.key_config.keys.home)
				{
					self.move_selection(ScrollType::Home)?
				} else if key_match(
					k,
					self.key_config.keys.shift_down,
				) || key_match(k, self.key_config.keys.end)
				{
					self.move_selection(ScrollType::End)?
				} else if key_match(k, self.key_config.keys.page_up) {
					self.move_selection(ScrollType::PageUp)?
				} else if key_match(k, self.key_config.keys.page_down)
				{
					self.move_selection(ScrollType::PageDown)?
				} else if key_match(
					k,
					self.key_config.keys.log_mark_commit,
				) {
					self.mark();
					true
				} else if key_match(
					k,
					self.key_config.keys.rebase_interactive,
				) && git_state == RepoState::Clean {
					if let Some(id) = self.get_last_selected_commit() {
						let base: CommitId = asyncgit::sync::parent_ids(
							&self.repo.borrow(),
							*id,
						)
						.unwrap()[0];
						self.queue.push(InternalEvent::RebaseInteractiveWithEditor(base));
					}
					true
				} else if key_match(
					k,
					self.key_config.keys.rebase_continue,
				) && git_state == RepoState::Rebase {
					self.queue.push(InternalEvent::RebaseInteractiveContinue);
					true
				} else if key_match(
					k,
					self.key_config.keys.rebase_skip,
				) && git_state == RepoState::Rebase {
					self.queue.push(InternalEvent::RebaseInteractiveSkip);
					true
				} else if key_match(
					k,
					self.key_config.keys.rebase_abort,
				) && git_state == RepoState::Rebase {
					self.queue.push(InternalEvent::RebaseInteractiveAbort);
					true
				} else if key_match(
					k,
					self.key_config.keys.delete_generic,
				) {
					if self.marked.is_empty() {
						self.mark();
					}
					if self.marked.is_empty() {
						self.queue.push(InternalEvent::ShowErrorMsg(
							String::from(
								"No commits selected to drop",
							),
						));
					} else {
						self.queue.push(
							InternalEvent::ConfirmCustom(
								CustomConfirmData {
									title: "Drop commits?"
										.to_string(),
									msg: self.get_marked_summary(),
									confirm: "drop".to_string(),
									q: self.local_queue.clone(),
								},
							),
						);
					}
					true
				} else if key_match(
					k,
					self.key_config.keys.rebase_fixup_marked,
				) {
					if self.marked.is_empty() {
						self.mark();
					}
					if self.marked.is_empty() {
						self.queue.push(InternalEvent::ShowErrorMsg(
							String::from(
								"No commits selected to fixup",
							),
						));
					} else {
						self.queue.push(
							InternalEvent::ConfirmCustom(
								CustomConfirmData {
									title: "FixUp commits?"
										.to_string(),
									msg: self.get_marked_summary(),
									confirm: "fixup".to_string(),
									q: self.local_queue.clone(),
								},
							),
						);
					}
					true
				} else if key_match(
					k,
					self.key_config.keys.cherrypick,
				) {
					if self.marked.is_empty() {
						self.mark();
					}
					if self.marked.is_empty() {
						self.queue.push(InternalEvent::ShowErrorMsg(
							String::from(
								"No commits selected to cherrypick",
							),
						));
					} else {
						self.queue.push(
							InternalEvent::ConfirmCustom(
								CustomConfirmData {
									title: "Cherrypick?".to_string(),
									msg: self.get_marked_summary(),
									confirm: "cherrypick".to_string(),
									q: self.local_queue.clone(),
								},
							),
						);
					}
					true
				} else if key_match(
					k,
					self.key_config.keys.log_checkout_commit,
				) {
					self.checkout();
					true
				} else if key_match(
					k,
					self.key_config.keys.start_search_forward_init,
				) {
					self.combo_state =
						KeyComboState::SearchInitForward;
					//self.show_search();
					true
				} else if key_match(
					k,
					self.key_config.keys.exit_popup,
				) {
					if self.search_field.is_visible()
						|| self.filter_field.is_visible()
					{
						self.stop_search();
						self.stop_filter();
					} else {
						self.update_path_filter(PathBuf::new());
						if !self.marked.is_empty() {
							self.marked.clear();
							self.queue.push(InternalEvent::Update(
								NeedsUpdate::ALL,
							));
						}
					}
					true
				} else if key_match(
					k,
					self.key_config.keys.filter_commits_init,
				) {
					self.combo_state = KeyComboState::FilterInit;
					//self.show_filter();
					true
				} else if key_match(
					k,
					self.key_config.keys.toggle_workarea,
				) {
					self.toggle_input_focus();
					true
				} else if key_match(
					k,
					self.key_config.keys.search_next,
				) {
					self.search_commit_forward();
					true
				} else if key_match(
					k,
					self.key_config.keys.search_prev,
				) {
					self.search_commit_backward();
					true
				} else if key_match(
					k,
					self.key_config.keys.fuzzy_find,
				) {
					let commit = get_head(&self.repo.borrow());
					match commit {
						Ok(commit) => {
							let r = asyncgit::sync::tree_files(
								&self.repo.borrow(),
								commit,
								true,
							);

							match r {
								Ok(v) => self.queue.push(
									InternalEvent::OpenFileFinder(
										v,
										Some(
											self.local_queue.clone(),
										),
									),
								),
								Err(e) => self.queue.push(
									InternalEvent::ShowErrorMsg(
										format!(
										"Could not get file list: {}",
										e
									),
									),
								),
							}
						}
						Err(e) => {
							self.queue.push(
								InternalEvent::ShowErrorMsg(format!(
									"Could not get file list: {}",
									e
								)),
							);
						}
					}
					true
				} else {
					false
				};
			return Ok(selection_changed.into());
		}
		Ok(EventState::NotConsumed)
	}

	fn search_input_event(
		&mut self,
		ev: &Event,
	) -> Result<EventState> {
		if let Event::Key(k) = ev {
			if key_match(k, self.key_config.keys.enter) {
				self.focused_field = Focused::List;
				self.current_search =
					self.search_field.get_text().to_string();
				//start actual search
				self.search_commit_forward();
				Ok(EventState::Consumed)
			} else if key_match(k, self.key_config.keys.exit_popup) {
				self.stop_search();
				Ok(EventState::Consumed)
			} else if key_match(
				k,
				self.key_config.keys.toggle_workarea,
			) {
				self.toggle_input_focus();
				Ok(EventState::Consumed)
			} else if key_match(
				k,
				self.key_config.keys.open_suboptions,
			) {
				self.search_options.show()?;
				Ok(EventState::Consumed)
			} else {
				self.search_field.event(ev)
			}
		} else {
			self.search_field.event(ev)
		}
	}

	fn filter_input_event(
		&mut self,
		ev: &Event,
	) -> Result<EventState> {
		if let Event::Key(k) = ev {
			if key_match(k, self.key_config.keys.enter) {
				self.focused_field = Focused::List;
				self.filter_updated = true;
				Ok(EventState::Consumed)
			} else if key_match(k, self.key_config.keys.exit_popup) {
				self.stop_filter();
				Ok(EventState::Consumed)
			} else if key_match(
				k,
				self.key_config.keys.toggle_workarea,
			) {
				self.toggle_input_focus();
				Ok(EventState::Consumed)
			} else if key_match(
				k,
				self.key_config.keys.open_suboptions,
			) {
				self.filter_options.show()?;
				Ok(EventState::Consumed)
			} else {
				self.filter_field.event(ev)
			}
		} else {
			self.filter_field.event(ev)
		}
	}

	fn draw_input_field<B: Backend>(
		&self,
		f: &mut Frame<B>,
		field: &TextInputComponent,
		title: &str,
		area: Rect,
		selected: bool,
	) -> Result<()> {
		if field.is_visible() {
			f.render_widget(
				Block::default()
					.borders(
						Borders::TOP | Borders::RIGHT | Borders::LEFT,
					)
					.title(Span::styled(
						title,
						self.theme.title(selected),
					))
					.border_style(self.theme.block(selected)),
				area,
			);
			let edit_area = Rect::new(
				area.x + 1,
				area.y + 1,
				area.width - 2,
				area.height - 1,
			);
			field.draw(f, edit_area)?;
		}
		Ok(())
	}

	pub fn is_list_focused(&self) -> bool {
		self.focused_field == Focused::List
	}
}

impl DrawableComponent for CommitList {
	fn draw<B: Backend>(
		&self,
		f: &mut Frame<B>,
		area: Rect,
	) -> Result<()> {
		let path_visible = !self.path_filter.as_os_str().is_empty();
		let original_area = area.clone();
		let v_size_path = if path_visible { 2 } else { 0 };
		let v_size_search =
			if self.search_field.is_visible() { 2 } else { 0 };
		let v_size_filter =
			if self.filter_field.is_visible() { 2 } else { 0 };
		let v_blocks = Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints(
				[
					Constraint::Length(v_size_path),
					Constraint::Length(v_size_filter),
					Constraint::Length(v_size_search),
					Constraint::Percentage(100),
				]
				.as_ref(),
			)
			.split(area);
		let path_area = v_blocks[0];
		let filter_area = v_blocks[1];
		let search_area = v_blocks[2];
		let list_area = v_blocks[3];

		if path_visible {
			let p_filter = self.path_filter.to_str().unwrap_or("");
			let path_ui = Paragraph::new(Span::raw(p_filter)).block(
				Block::default()
					.borders(
						Borders::TOP | Borders::RIGHT | Borders::LEFT,
					)
					.border_style(self.theme.block(false)),
			);
			f.render_widget(path_ui, path_area);
		}

		self.draw_input_field(
			f,
			&self.filter_field,
			&self.filter_options.title,
			filter_area,
			self.focused_field == Focused::InputFilter,
		)?;

		self.draw_input_field(
			f,
			&self.search_field,
			&self.search_options.title,
			search_area,
			self.focused_field == Focused::InputSearch,
		)?;

		let area = list_area;
		let list_focused = self.focused_field == Focused::List;

		let current_size = (
			area.width.saturating_sub(2),
			area.height.saturating_sub(2),
		);
		self.current_size.set(Some(current_size));

		let height_in_lines = current_size.1 as usize;
		let selection = self.relative_selection();

		self.scroll_top.set(calc_scroll_top(
			self.scroll_top.get(),
			height_in_lines,
			selection,
		));

		let title = format!(
			"{} {}/{}",
			self.title,
			self.count_total.saturating_sub(self.selection),
			self.count_total,
		);

		f.render_widget(
			Paragraph::new(
				self.get_text(
					height_in_lines,
					current_size.0 as usize,
				),
			)
			.block(
				Block::default()
					.borders(Borders::ALL)
					.title(Span::styled(
						title.as_str(),
						self.theme.title(
							list_focused && self.external_focus,
						),
					))
					.border_style(
						self.theme.block(
							list_focused && self.external_focus,
						),
					),
			)
			.alignment(Alignment::Left),
			area,
		);

		draw_scrollbar(
			f,
			area,
			&self.theme,
			self.count_total,
			self.selection,
			Orientation::Vertical,
		);

		if self.search_options.is_visible() {
			self.search_options.draw(f, original_area)?;
		} else if self.filter_options.is_visible() {
			self.filter_options.draw(f, original_area)?;
		}

		Ok(())
	}
}

impl Component for CommitList {
	fn event(&mut self, ev: &Event) -> Result<EventState> {
		if self.search_options.is_visible() {
			self.search_options.event(ev)?;
			return Ok(EventState::Consumed);
		} else if self.filter_options.is_visible() {
			self.filter_options.event(ev)?;
			return Ok(EventState::Consumed);
		}
		match self.focused_field {
			Focused::List => self.list_event(ev),
			Focused::InputSearch => self.search_input_event(ev),
			Focused::InputFilter => self.filter_input_event(ev),
		}
	}

	fn commands(
		&self,
		out: &mut Vec<CommandInfo>,
		_force_all: bool,
	) -> CommandBlocking {
		if self.search_options.is_visible() {
			return self.search_options.commands(out, _force_all);
		}
		if self.filter_options.is_visible() {
			return self.filter_options.commands(out, _force_all);
		}
		out.push(CommandInfo::new(
			strings::commands::scroll(&self.key_config),
			self.selected_entry().is_some(),
			self.combo_state == KeyComboState::Empty,
		));
		out.push(CommandInfo::new(
			strings::commands::commit_list_mark(
				&self.key_config,
				self.selected_entry_marked(),
			),
			true,
			self.combo_state == KeyComboState::Empty,
		));
		out.push(CommandInfo::new(
			strings::commands::filter_by_path(&self.key_config),
			true,
			self.combo_state == KeyComboState::Empty,
		));
		out.push(CommandInfo::new(
			strings::commands::start_search(&self.key_config),
			true,
			self.combo_state == KeyComboState::Empty,
		));
		out.push(CommandInfo::new(
			strings::commands::start_filter(&self.key_config),
			true,
			self.combo_state == KeyComboState::Empty,
		));
		out.push(CommandInfo::new(
			strings::commands::search_all(&self.key_config),
			true,
			self.combo_state == KeyComboState::SearchInitForward,
		));
		out.push(CommandInfo::new(
			strings::commands::search_author(&self.key_config),
			true,
			self.combo_state == KeyComboState::SearchInitForward,
		));
		out.push(CommandInfo::new(
			strings::commands::search_msg(&self.key_config),
			true,
			self.combo_state == KeyComboState::SearchInitForward,
		));
		out.push(CommandInfo::new(
			strings::commands::search_sha(&self.key_config),
			true,
			self.combo_state == KeyComboState::SearchInitForward,
		));
		out.push(CommandInfo::new(
			strings::commands::filter_all(&self.key_config),
			true,
			self.combo_state == KeyComboState::FilterInit,
		));
		out.push(CommandInfo::new(
			strings::commands::filter_author(&self.key_config),
			true,
			self.combo_state == KeyComboState::FilterInit,
		));
		out.push(CommandInfo::new(
			strings::commands::filter_msg(&self.key_config),
			true,
			self.combo_state == KeyComboState::FilterInit,
		));
		let git_state = sync::repo_state(&self.repo.borrow())
			.unwrap_or(RepoState::Clean);
		let rebase_active = git_state == RepoState::Rebase;
		let is_clean = git_state == RepoState::Clean;
		out.push(CommandInfo::new(
			strings::commands::rebase_interactive(&self.key_config),
			self.is_list_focused() && is_clean,
			self.is_list_focused() && is_clean,
		));
		out.push(CommandInfo::new(
			strings::commands::abort_rebase(&self.key_config),
			rebase_active,
			rebase_active,
		));
		out.push(CommandInfo::new(
			strings::commands::continue_rebase(&self.key_config),
			rebase_active,
			rebase_active,
		));
		out.push(CommandInfo::new(
			strings::commands::skip_rebase(&self.key_config),
			rebase_active,
			rebase_active,
		));
		out.push(CommandInfo::new(
			strings::commands::rebase_fixup(&self.key_config),
			self.is_list_focused() && is_clean,
			self.is_list_focused() && is_clean
		));

		if self.combo_state == KeyComboState::Empty && git_state != RepoState::Rebase {
			CommandBlocking::PassingOn
		} else {
			CommandBlocking::Blocking
		}
	}

	fn focus(&mut self, _focus: bool) {
		self.external_focus = _focus;
	}

	fn focused(&self) -> bool {
		self.external_focus
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_string_width_align() {
		assert_eq!(string_width_align("123", 3), "123");
		assert_eq!(string_width_align("123", 2), "..");
		assert_eq!(string_width_align("123", 3), "123");
		assert_eq!(string_width_align("12345", 6), "12345 ");
		assert_eq!(string_width_align("1234556", 4), "12..");
	}

	#[test]
	fn test_string_width_align_unicode() {
		assert_eq!(string_width_align("äste", 3), "ä..");
		assert_eq!(
			string_width_align("wüsten äste", 10),
			"wüsten ä.."
		);
		assert_eq!(
			string_width_align("Jon Grythe Stødle", 19),
			"Jon Grythe Stødle  "
		);
	}
}
