use crate::{
	components::{
		AppOption, BlameFileOpen, CopyClipboardOpen, FileRevOpen,
		FileTreeOpen, InspectCommitOpen,
	},
	tabs::StashingOptions,
};
use asyncgit::{
	sync::{diff::DiffLinePosition, BranchInfo, CommitId, TreeFile},
	PushType,
};
use bitflags::bitflags;
use std::{
	cell::RefCell, collections::VecDeque, path::PathBuf, rc::Rc,
};

bitflags! {
	/// flags defining what part of the app need to update
	pub struct NeedsUpdate: u32 {
		/// app::update
		const ALL = 0b001;
		/// diff may have changed (app::update_diff)
		const DIFF = 0b010;
		/// commands might need updating (app::update_commands)
		const COMMANDS = 0b100;
		/// branches have changed
		const BRANCHES = 0b1000;
	}
}

pub enum LocalEvent {
	PickBranch(BranchInfo),
}

pub type LocalQueue = VecDeque<LocalEvent>;
pub type SharedLocalQueue = Rc<RefCell<LocalQueue>>;

pub fn create_local_queue() -> SharedLocalQueue {
	Rc::new(RefCell::new(LocalQueue::new()))
}

/// data of item that is supposed to be reset
pub struct ResetItem {
	/// path to the item (folder/file)
	pub path: String,
	/// are talking about a folder here? otherwise it's a single file
	pub is_folder: bool,
}

///
pub enum Action {
	Reset(ResetItem),
	ResetHunk(String, u64),
	ResetLines(String, Vec<DiffLinePosition>),
	StashDrop(Vec<CommitId>),
	StashPop(CommitId),
	DeleteLocalBranch(String),
	DeleteRemoteBranch(String),
	DeleteTag(String),
	DeleteRemoteTag(String, String),
	ForcePush(String, bool),
	PullMerge { incoming: usize, rebase: bool },
	AbortMerge,
	AbortRebase,
	AbortRevert,
}

#[derive(Debug)]
pub enum StackablePopupOpen {
	///
	BlameFile(BlameFileOpen),
	///
	FileRevlog(FileRevOpen),
	///
	FileTree(FileTreeOpen),
	///
	InspectCommit(InspectCommitOpen),
	///
	CompareCommits(InspectCommitOpen),
	///
	CopyClipboardCommit(CopyClipboardOpen),
}

///
pub enum InternalEvent {
	///
	ConfirmAction(Action),
	///
	ConfirmedAction(Action),
	///
	ShowErrorMsg(String),
	///
	ShowInfoMsg(String),
	///
	Update(NeedsUpdate),
	///
	StatusLastFileMoved,
	/// open commit msg input
	OpenCommit,
	/// open external editor to edit commit msg and commit
	CommitWithExternalEditor,
	///
	PopupStashing(StashingOptions),
	///
	TabSwitchStatus,
	///
	SelectCommitInRevlog(CommitId),
	///
	TagCommit(CommitId),
	///
	Tags,
	///
	CreateBranch,
	///
	RenameBranch(String, String),
	///
	SelectBranch,
	///
	PickBranch(SharedLocalQueue),
	///
	OpenExternalEditor(Option<String>),
	///
	Push(String, PushType, bool, bool),
	///
	Pull(String),
	///
	PushTags,
	///
	OptionSwitched(AppOption),
	///
	OpenFileFinder(Vec<TreeFile>),
	///
	OpenBranchFinder(Vec<String>),
	///
	FileFinderChanged(Option<PathBuf>),
	///
	BranchFinderChanged(Option<usize>),
	///
	FetchRemotes,
	///
	OpenPopup(StackablePopupOpen),
	///
	PopupStackPop,
	///
	PopupStackPush(StackablePopupOpen),
	///
	ViewSubmodules,
	///
	OpenRepo { path: PathBuf },
	///
	OpenResetPopup(CommitId),
	///
	RewordCommit(CommitId),
}

/// single threaded simple queue for components to communicate with each other
#[derive(Clone)]
pub struct Queue {
	data: Rc<RefCell<VecDeque<InternalEvent>>>,
}

impl Queue {
	pub fn new() -> Self {
		Self {
			data: Rc::new(RefCell::new(VecDeque::new())),
		}
	}

	pub fn push(&self, ev: InternalEvent) {
		self.data.borrow_mut().push_back(ev);
	}

	pub fn pop(&self) -> Option<InternalEvent> {
		self.data.borrow_mut().pop_front()
	}

	pub fn clear(&self) {
		self.data.borrow_mut().clear();
	}
}
