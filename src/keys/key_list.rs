use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::{fs::File, path::PathBuf};
use struct_patch::traits::Patch as PatchTrait;
use struct_patch::Patch;

#[derive(Debug, PartialOrd, Clone, Copy, Serialize, Deserialize)]
pub struct GituiKeyEvent {
	pub code: KeyCode,
	pub modifiers: KeyModifiers,
}

impl GituiKeyEvent {
	pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
		Self { code, modifiers }
	}
}

pub fn key_match(ev: &KeyEvent, binding: GituiKeyEvent) -> bool {
	ev.code == binding.code && ev.modifiers == binding.modifiers
}

impl PartialEq for GituiKeyEvent {
	fn eq(&self, other: &Self) -> bool {
		let ev: KeyEvent = self.into();
		let other: KeyEvent = other.into();
		ev == other
	}
}

impl From<&GituiKeyEvent> for KeyEvent {
	fn from(other: &GituiKeyEvent) -> Self {
		Self::new(other.code, other.modifiers)
	}
}

impl From<&KeyEvent> for GituiKeyEvent {
	fn from(other: &KeyEvent) -> Self {
		Self::new(other.code, other.modifiers)
	}
}

#[derive(Clone, Patch)]
#[patch_derive(Deserialize)]
pub struct KeysList {
	pub tab_status: GituiKeyEvent,
	pub tab_log: GituiKeyEvent,
	pub tab_files: GituiKeyEvent,
	pub tab_stashing: GituiKeyEvent,
	pub tab_stashes: GituiKeyEvent,
	pub tab_toggle: GituiKeyEvent,
	pub tab_toggle_reverse: GituiKeyEvent,
	pub toggle_workarea: GituiKeyEvent,
	pub exit: GituiKeyEvent,
	pub quit: GituiKeyEvent,
	pub exit_popup: GituiKeyEvent,
	pub open_commit: GituiKeyEvent,
	pub commit_with_editor: GituiKeyEvent,
	pub open_commit_editor: GituiKeyEvent,
	pub open_help: GituiKeyEvent,
	pub open_options: GituiKeyEvent,
	pub move_left: GituiKeyEvent,
	pub move_right: GituiKeyEvent,
	pub move_up: GituiKeyEvent,
	pub move_down: GituiKeyEvent,
	pub tree_collapse_recursive: GituiKeyEvent,
	pub tree_expand_recursive: GituiKeyEvent,
	pub home: GituiKeyEvent,
	pub end: GituiKeyEvent,
	pub popup_up: GituiKeyEvent,
	pub popup_down: GituiKeyEvent,
	pub page_down: GituiKeyEvent,
	pub page_up: GituiKeyEvent,
	pub shift_up: GituiKeyEvent,
	pub shift_down: GituiKeyEvent,
	pub enter: GituiKeyEvent,
	pub blame: GituiKeyEvent,
	pub file_history: GituiKeyEvent,
	pub edit_file: GituiKeyEvent,
	pub status_stage_all: GituiKeyEvent,
	pub status_reset_item: GituiKeyEvent,
	pub status_ignore_file: GituiKeyEvent,
	pub diff_stage_lines: GituiKeyEvent,
	pub diff_reset_lines: GituiKeyEvent,
	pub stashing_save: GituiKeyEvent,
	pub stashing_toggle_untracked: GituiKeyEvent,
	pub stashing_toggle_index: GituiKeyEvent,
	pub stash_apply: GituiKeyEvent,
	pub stash_open: GituiKeyEvent,
	pub stash_drop: GituiKeyEvent,
	pub cmd_bar_toggle: GituiKeyEvent,
	pub log_tag_commit: GituiKeyEvent,
	pub log_mark_commit: GituiKeyEvent,
	pub log_checkout_commit: GituiKeyEvent,
	pub log_reset_comit: GituiKeyEvent,
	pub log_reword_comit: GituiKeyEvent,
	pub commit_amend: GituiKeyEvent,
	pub toggle_verify: GituiKeyEvent,
	pub copy: GituiKeyEvent,
	pub copy_hunk: GituiKeyEvent,
	pub create_branch: GituiKeyEvent,
	pub rename_branch: GituiKeyEvent,
	pub select_branch: GituiKeyEvent,
	pub delete_branch: GituiKeyEvent,
	pub merge_branch: GituiKeyEvent,
	pub rebase_branch: GituiKeyEvent,
	pub rebase_abort: GituiKeyEvent,
	pub rebase_interactive: GituiKeyEvent,
	pub rebase_skip: GituiKeyEvent,
	pub rebase_continue: GituiKeyEvent,
	pub rebase_fixup_marked: GituiKeyEvent,
	pub compare_commits: GituiKeyEvent,
	pub tags: GituiKeyEvent,
	pub delete_tag: GituiKeyEvent,
	pub delete_generic: GituiKeyEvent,
	pub select_tag: GituiKeyEvent,
	pub push: GituiKeyEvent,
	pub open_file_tree: GituiKeyEvent,
	pub force_push: GituiKeyEvent,
	pub fetch: GituiKeyEvent,
	pub pull: GituiKeyEvent,
	pub abort_merge: GituiKeyEvent,
	pub undo_commit: GituiKeyEvent,
	pub stage_unstage_item: GituiKeyEvent,
	pub tag_annotate: GituiKeyEvent,
	pub view_submodules: GituiKeyEvent,
	pub view_submodule_parent: GituiKeyEvent,
	pub update_submodule: GituiKeyEvent,
	pub commit_history_next: GituiKeyEvent,
	pub copy_clipboard_sha: GituiKeyEvent,
	pub copy_clipboard_email: GituiKeyEvent,
	pub copy_clipboard_author: GituiKeyEvent,
	pub copy_clipboard_message: GituiKeyEvent,
	pub copy_clipboard_summary: GituiKeyEvent,
	pub run_external_command: GituiKeyEvent,
	pub start_search_forward_init: GituiKeyEvent,
	pub start_search_backward_init: GituiKeyEvent,
	pub search_filter_author: GituiKeyEvent,
	pub search_filter_msg: GituiKeyEvent,
	pub search_sha: GituiKeyEvent,
	pub search_next: GituiKeyEvent,
	pub search_prev: GituiKeyEvent,
	pub open_suboptions: GituiKeyEvent,
	pub filter_commits_init: GituiKeyEvent,
	pub assign_shortcut: GituiKeyEvent,
	pub clear_shortcut: GituiKeyEvent,
	pub clear_all_shortcut: GituiKeyEvent,
	pub trigger_branch_shortcut: GituiKeyEvent,
	pub toggle_split: GituiKeyEvent,
	pub cherrypick: GituiKeyEvent,
	pub fuzzy_find: GituiKeyEvent,
	pub generic_push: GituiKeyEvent,
	pub generic_pop: GituiKeyEvent,
}

#[rustfmt::skip]
impl Default for KeysList {
	fn default() -> Self {
		Self {
			tab_status: GituiKeyEvent::new(KeyCode::Char('1'), KeyModifiers::empty()),
			tab_log: GituiKeyEvent::new(KeyCode::Char('2'),  KeyModifiers::empty()),
			tab_files: GituiKeyEvent::new(KeyCode::Char('3'),  KeyModifiers::empty()),
			tab_stashing: GituiKeyEvent::new(KeyCode::Char('4'),  KeyModifiers::empty()),
			tab_stashes: GituiKeyEvent::new(KeyCode::Char('5'),  KeyModifiers::empty()),
			tab_toggle: GituiKeyEvent::new(KeyCode::Tab,  KeyModifiers::empty()),
			tab_toggle_reverse: GituiKeyEvent::new(KeyCode::BackTab,  KeyModifiers::SHIFT),
			toggle_workarea: GituiKeyEvent::new(KeyCode::Char('w'),  KeyModifiers::empty()),
			exit: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::CONTROL),
			quit: GituiKeyEvent::new(KeyCode::Char('q'),  KeyModifiers::empty()),
			exit_popup: GituiKeyEvent::new(KeyCode::Esc,  KeyModifiers::empty()),
			open_commit: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::empty()),
			commit_with_editor: GituiKeyEvent::new(KeyCode::Char('C'),  KeyModifiers::SHIFT),
			open_commit_editor: GituiKeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
			open_help: GituiKeyEvent::new(KeyCode::Char('h'),  KeyModifiers::empty()),
			open_options: GituiKeyEvent::new(KeyCode::Char('o'),  KeyModifiers::empty()),
			move_left: GituiKeyEvent::new(KeyCode::Left,  KeyModifiers::empty()),
			move_right: GituiKeyEvent::new(KeyCode::Right,  KeyModifiers::empty()),
			tree_collapse_recursive: GituiKeyEvent::new(KeyCode::Left,  KeyModifiers::SHIFT),
			tree_expand_recursive: GituiKeyEvent::new(KeyCode::Right,  KeyModifiers::SHIFT),
			home: GituiKeyEvent::new(KeyCode::Home,  KeyModifiers::empty()),
			end: GituiKeyEvent::new(KeyCode::End,  KeyModifiers::empty()),
			move_up: GituiKeyEvent::new(KeyCode::Up,  KeyModifiers::empty()),
			move_down: GituiKeyEvent::new(KeyCode::Down,  KeyModifiers::empty()),
			popup_up: GituiKeyEvent::new(KeyCode::Up,  KeyModifiers::empty()),
			popup_down: GituiKeyEvent::new(KeyCode::Down,  KeyModifiers::empty()),
			page_down: GituiKeyEvent::new(KeyCode::PageDown,  KeyModifiers::empty()),
			page_up: GituiKeyEvent::new(KeyCode::PageUp,  KeyModifiers::empty()),
			shift_up: GituiKeyEvent::new(KeyCode::Up,  KeyModifiers::SHIFT),
			shift_down: GituiKeyEvent::new(KeyCode::Down,  KeyModifiers::SHIFT),
			enter: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
			blame: GituiKeyEvent::new(KeyCode::Char('b'),  KeyModifiers::empty()),
			file_history: GituiKeyEvent::new(KeyCode::Char('I'),  KeyModifiers::SHIFT),
			edit_file: GituiKeyEvent::new(KeyCode::Char('e'),  KeyModifiers::empty()),
			status_stage_all: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::empty()),
			status_reset_item: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			diff_reset_lines: GituiKeyEvent::new(KeyCode::Char('d'),  KeyModifiers::empty()),
			status_ignore_file: GituiKeyEvent::new(KeyCode::Char('i'),  KeyModifiers::empty()),
			diff_stage_lines: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			stashing_save: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			stashing_toggle_untracked: GituiKeyEvent::new(KeyCode::Char('u'),  KeyModifiers::empty()),
			stashing_toggle_index: GituiKeyEvent::new(KeyCode::Char('i'),  KeyModifiers::empty()),
			stash_apply: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::empty()),
			stash_open: GituiKeyEvent::new(KeyCode::Right,  KeyModifiers::empty()),
			stash_drop: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			cmd_bar_toggle: GituiKeyEvent::new(KeyCode::Char('.'),  KeyModifiers::empty()),
			log_tag_commit: GituiKeyEvent::new(KeyCode::Char('t'),  KeyModifiers::empty()),
			log_mark_commit: GituiKeyEvent::new(KeyCode::Char(' '),  KeyModifiers::empty()),
			log_checkout_commit: GituiKeyEvent { code: KeyCode::Char('S'), modifiers: KeyModifiers::SHIFT },
			log_reset_comit: GituiKeyEvent { code: KeyCode::Char('R'), modifiers: KeyModifiers::SHIFT },
			log_reword_comit: GituiKeyEvent { code: KeyCode::Char('r'), modifiers: KeyModifiers::empty() },
			commit_amend: GituiKeyEvent::new(KeyCode::Char('A'),  KeyModifiers::SHIFT),
			toggle_verify: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::CONTROL),
			copy: GituiKeyEvent::new(KeyCode::Char('y'),  KeyModifiers::empty()),
			copy_hunk: GituiKeyEvent::new(KeyCode::Char('h'),  KeyModifiers::empty()),
			create_branch: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::empty()),
			rename_branch: GituiKeyEvent::new(KeyCode::Char('r'),  KeyModifiers::empty()),
			select_branch: GituiKeyEvent::new(KeyCode::Char('b'),  KeyModifiers::empty()),
			delete_branch: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			merge_branch: GituiKeyEvent::new(KeyCode::Char('m'),  KeyModifiers::empty()),
			rebase_branch: GituiKeyEvent::new(KeyCode::Char('R'),  KeyModifiers::SHIFT),
			rebase_interactive: GituiKeyEvent::new(KeyCode::Char('I'),  KeyModifiers::SHIFT),
			rebase_skip: GituiKeyEvent::new(KeyCode::Char('S'),  KeyModifiers::SHIFT),
			rebase_continue: GituiKeyEvent::new(KeyCode::Char('C'),  KeyModifiers::SHIFT),
			rebase_abort: GituiKeyEvent::new(KeyCode::Char('A'),  KeyModifiers::SHIFT),
			rebase_fixup_marked: GituiKeyEvent::new(KeyCode::Char('F'),  KeyModifiers::SHIFT),
			compare_commits: GituiKeyEvent::new(KeyCode::Char('C'),  KeyModifiers::SHIFT),
			tags: GituiKeyEvent::new(KeyCode::Char('T'),  KeyModifiers::SHIFT),
			delete_tag: GituiKeyEvent::new(KeyCode::Char('D'),  KeyModifiers::SHIFT),
			delete_generic: GituiKeyEvent::new(KeyCode::Char('d'),  KeyModifiers::empty()),
			select_tag: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
			push: GituiKeyEvent::new(KeyCode::Char('p'),  KeyModifiers::empty()),
			force_push: GituiKeyEvent::new(KeyCode::Char('P'),  KeyModifiers::SHIFT),
			undo_commit: GituiKeyEvent::new(KeyCode::Char('U'),  KeyModifiers::SHIFT),
			fetch: GituiKeyEvent::new(KeyCode::Char('F'),  KeyModifiers::SHIFT),
			pull: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::empty()),
			abort_merge: GituiKeyEvent::new(KeyCode::Char('A'),  KeyModifiers::SHIFT),
			open_file_tree: GituiKeyEvent::new(KeyCode::Char('F'),  KeyModifiers::SHIFT),
			stage_unstage_item: GituiKeyEvent::new(KeyCode::Enter,  KeyModifiers::empty()),
			tag_annotate: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::CONTROL),
			view_submodules: GituiKeyEvent::new(KeyCode::Char('S'),  KeyModifiers::SHIFT),
			view_submodule_parent: GituiKeyEvent::new(KeyCode::Char('p'),  KeyModifiers::empty()),
			update_submodule: GituiKeyEvent::new(KeyCode::Char('u'),  KeyModifiers::empty()),
			commit_history_next: GituiKeyEvent::new(KeyCode::Char('n'),  KeyModifiers::CONTROL),
			copy_clipboard_sha: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			copy_clipboard_email: GituiKeyEvent::new(KeyCode::Char('e'),  KeyModifiers::empty()),
			copy_clipboard_author: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::empty()),
			copy_clipboard_message: GituiKeyEvent::new(KeyCode::Char('m'),  KeyModifiers::empty()),
			copy_clipboard_summary: GituiKeyEvent::new(KeyCode::Char('S'),  KeyModifiers::SHIFT),
			run_external_command: GituiKeyEvent::new(KeyCode::Char(':'),  KeyModifiers::SHIFT),
			start_search_forward_init: GituiKeyEvent::new(KeyCode::Char('/'),  KeyModifiers::empty()),
			start_search_backward_init: GituiKeyEvent::new(KeyCode::Char('?'),  KeyModifiers::empty()),
			search_filter_author: GituiKeyEvent::new(KeyCode::Char('a'),  KeyModifiers::empty()),
			search_filter_msg: GituiKeyEvent::new(KeyCode::Char('m'),  KeyModifiers::empty()),
			search_sha: GituiKeyEvent::new(KeyCode::Char('s'),  KeyModifiers::empty()),
			search_next: GituiKeyEvent::new(KeyCode::Char('n'),  KeyModifiers::empty()),
			search_prev: GituiKeyEvent::new(KeyCode::Char('N'),  KeyModifiers::SHIFT),
			open_suboptions: GituiKeyEvent::new(KeyCode::Char('o'),  KeyModifiers::CONTROL),
			filter_commits_init: GituiKeyEvent::new(KeyCode::Char('='),  KeyModifiers::empty()),
			assign_shortcut: GituiKeyEvent::new(KeyCode::Char('='),  KeyModifiers::empty()),
			clear_shortcut: GituiKeyEvent::new(KeyCode::Char('x'),  KeyModifiers::empty()),
			clear_all_shortcut: GituiKeyEvent::new(KeyCode::Char('X'),  KeyModifiers::SHIFT),
			trigger_branch_shortcut: GituiKeyEvent::new(KeyCode::Char('b'),  KeyModifiers::empty()),
            toggle_split: GituiKeyEvent::new(KeyCode::Char('|'),  KeyModifiers::empty()),
            cherrypick: GituiKeyEvent::new(KeyCode::Char('c'),  KeyModifiers::empty()),
			fuzzy_find: GituiKeyEvent::new(KeyCode::Char('f'),  KeyModifiers::empty()),
			generic_push: GituiKeyEvent::new(KeyCode::Char('p'),  KeyModifiers::empty()),
			generic_pop: GituiKeyEvent::new(KeyCode::Char('P'),  KeyModifiers::SHIFT),
		}
	}
}

impl KeysList {
	pub fn init(file: PathBuf) -> Self {
		let mut keys_list = Self::default();
		if let Ok(f) = File::open(file) {
			if let Ok(patch) = ron::de::from_reader(f) {
				keys_list.apply(patch);
			}
		}
		keys_list
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;
	use std::io::Write;
	use tempfile::NamedTempFile;

	#[test]
	fn test_apply_vim_style_example() {
		let mut keys_list = KeysList::default();
		let f = File::open("vim_style_key_config.ron")
			.expect("vim style config should exist");
		let patch = ron::de::from_reader(f)
			.expect("vim style config format incorrect");
		keys_list.apply(patch);
	}

	#[test]
	fn test_smoke() {
		let mut file = NamedTempFile::new().unwrap();

		writeln!(
			file,
			r"
(
	move_down: Some(( code: Char('j'), modifiers: ( bits: 2,),)),
)
"
		)
		.unwrap();

		let keys = KeysList::init(file.path().to_path_buf());

		assert_eq!(keys.move_right, KeysList::default().move_right);
		assert_eq!(
			keys.move_down,
			GituiKeyEvent::new(
				KeyCode::Char('j'),
				KeyModifiers::CONTROL
			)
		);
	}
}
